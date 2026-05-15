//! Cursor (the AI editor, <https://cursor.com>) provider.
//!
//! Cursor stores its composer/chat history in a `SQLite` database under
//! `<config>/User/globalStorage/state.vscdb`. The relevant table is
//! `cursorDiskKV (key TEXT, value BLOB)` where:
//!
//! - `composerData:<composerId>`  → JSON session header
//! - `bubbleId:<composerId>:<bubbleId>` → JSON message bubble
//!
//! The bubble `type` field encodes the role: `1` = user, `2` = assistant.
//! Tool calls live under `toolFormerData` (legacy) or `tools`/`toolCalls`
//! (newer schema variants); we try both.
//!
//! Cursor has shipped several format variants — fields are tolerant: missing
//! fields skip rather than fail, and unparseable values are logged but never
//! crash discovery.

use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::Connection;
use serde::Deserialize;

use super::{HistoryProvider, ProviderError};
use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, ToolCall, ToolResult,
};
use crate::provider::claude_code::parse_text_with_code_blocks;

pub struct CursorProvider {
    dirs: Vec<PathBuf>,
}

impl CursorProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| state_db_path(d).exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

/// Cursor base directories, ordered by likelihood. Each entry is the parent
/// directory holding `User/globalStorage/state.vscdb`. Honors `AGHIST_HOME`
/// for testability and `CURSOR_HOME` as a power-user override.
fn base_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();

    if let Ok(cursor_home) = std::env::var("CURSOR_HOME") {
        result.push(PathBuf::from(cursor_home));
    }

    if let Some(home) = super::home_dir() {
        // Linux: ~/.config/Cursor
        result.push(home.join(".config").join("Cursor"));
        // macOS: ~/Library/Application Support/Cursor
        result.push(
            home.join("Library")
                .join("Application Support")
                .join("Cursor"),
        );
        // Windows: %APPDATA%\Cursor (mirrored under home for AGHIST_HOME tests)
        result.push(home.join("AppData").join("Roaming").join("Cursor"));
    }

    if std::env::var("AGHIST_HOME").is_err() {
        if let Some(base) = directories::BaseDirs::new() {
            let appdata = base.config_dir().join("Cursor");
            if !result.iter().any(|p| p == &appdata) {
                result.push(appdata);
            }
        }
    }

    result
}

fn state_db_path(base: &Path) -> PathBuf {
    base.join("User").join("globalStorage").join("state.vscdb")
}

impl HistoryProvider for CursorProvider {
    fn provider(&self) -> Provider {
        Provider::Cursor
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();

        for base in &self.dirs {
            let db_path = state_db_path(base);
            if !db_path.exists() {
                continue;
            }

            match read_sessions(&db_path) {
                Ok(found) => sessions.extend(found),
                Err(e) => {
                    tracing::warn!(path = %db_path.display(), error = %e, "skipping unreadable Cursor db");
                }
            }
        }

        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        // source_path points to the state.vscdb file itself — set by discover_sessions.
        load_messages_from_db(&session.source_path, &session.id.0)
    }
}

#[derive(Debug, Deserialize)]
struct ComposerData {
    #[serde(rename = "composerId")]
    composer_id: Option<String>,
    name: Option<String>,
    #[serde(rename = "lastUpdatedAt")]
    last_updated_at: Option<i64>,
    #[serde(rename = "createdAt")]
    created_at: Option<i64>,
    /// Newer Cursor builds embed bubble headers in the composer record.
    /// Each entry has `bubbleId` and a `type` (1 = user, 2 = assistant).
    #[serde(rename = "fullConversationHeadersOnly")]
    headers: Option<Vec<HeaderEntry>>,
    /// Some builds expose the working directory directly.
    #[serde(rename = "currentWorkspaceFolder")]
    workspace_folder: Option<String>,
    /// Newer schema may carry the model name on the composer record.
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HeaderEntry {
    #[serde(rename = "bubbleId")]
    bubble_id: Option<String>,
    #[serde(rename = "type")]
    bubble_type: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct BubbleData {
    #[serde(rename = "type")]
    bubble_type: Option<u8>,
    text: Option<String>,
    /// Older Cursor builds put the message body under `richText` markdown.
    #[serde(rename = "richText")]
    rich_text: Option<String>,
    /// Inline code blocks attached to the bubble.
    #[serde(rename = "codeBlocks", default)]
    code_blocks: Vec<CodeBlockData>,
    /// Legacy single tool-call structure.
    #[serde(rename = "toolFormerData")]
    tool_former: Option<ToolFormerData>,
    /// Newer multi-tool-call structure.
    #[serde(default, rename = "toolCalls")]
    tool_calls: Vec<ToolCallData>,
    /// Per-bubble timestamp (newer builds).
    #[serde(rename = "createdAt")]
    created_at: Option<i64>,
    /// Per-bubble model attribution (newer builds).
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodeBlockData {
    #[serde(rename = "languageId")]
    language: Option<String>,
    code: Option<String>,
    /// Some builds use `content` for the code body.
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolFormerData {
    #[serde(rename = "toolCallId")]
    tool_call_id: Option<String>,
    name: Option<String>,
    /// Cursor stores arguments as a JSON object; we serialize back to a
    /// string for the unified `ToolCall.arguments` slot.
    #[serde(default)]
    params: serde_json::Value,
    /// Free-form result text. Schema varies — we tolerate either a string
    /// or a structured object.
    #[serde(default)]
    result: serde_json::Value,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolCallData {
    id: Option<String>,
    name: Option<String>,
    #[serde(default)]
    arguments: serde_json::Value,
    #[serde(default)]
    result: serde_json::Value,
    status: Option<String>,
}

fn millis_to_datetime(millis: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(millis).single()
}

fn read_sessions(db_path: &Path) -> Result<Vec<Session>, ProviderError> {
    let conn = open_readonly(db_path)?;

    // The cursorDiskKV table may not exist on a fresh install — treat
    // missing-table as zero sessions rather than an error.
    if !table_exists(&conn, "cursorDiskKV")? {
        return Ok(Vec::new());
    }

    let mut stmt = conn
        .prepare("SELECT key, value FROM cursorDiskKV WHERE key LIKE 'composerData:%'")
        .map_err(sql_err(db_path))?;

    let rows = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            let value: Vec<u8> = row.get(1)?;
            Ok((key, value))
        })
        .map_err(sql_err(db_path))?;

    let mut sessions = Vec::new();
    let mut row_count: usize = 0;
    let mut parse_failures: usize = 0;
    for row in rows {
        let (key, value) = match row {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "skipping malformed Cursor row");
                continue;
            }
        };
        row_count += 1;
        match build_session_from_row(&key, &value, db_path) {
            Some(s) => sessions.push(s),
            None => parse_failures += 1,
        }
    }
    tracing::info!(
        path = %db_path.display(),
        rows = row_count,
        parse_failures,
        sessions = sessions.len(),
        "Cursor session discovery complete"
    );
    Ok(sessions)
}

fn build_session_from_row(key: &str, value: &[u8], db_path: &Path) -> Option<Session> {
    let composer_id = key.strip_prefix("composerData:")?.to_string();

    let raw: ComposerData = match serde_json::from_slice(value) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(key = %key, error = %e, "failed to parse composer JSON");
            return None;
        }
    };

    let id = raw.composer_id.unwrap_or(composer_id);

    let started_at = raw
        .created_at
        .and_then(millis_to_datetime)
        .or_else(|| raw.last_updated_at.and_then(millis_to_datetime))?;
    let ended_at = raw.last_updated_at.and_then(millis_to_datetime);

    let project_path = raw.workspace_folder.clone().map(PathBuf::from);
    let project_name = raw
        .workspace_folder
        .as_deref()
        .and_then(super::project_name_from_path);

    let message_count = raw
        .headers
        .as_ref()
        .map_or(0, |h| h.iter().filter(|e| e.bubble_id.is_some()).count());

    Some(Session {
        id: SessionId(id),
        provider: Provider::Cursor,
        project_path,
        project_name,
        git_branch: None,
        started_at,
        ended_at,
        summary: raw.name,
        model: raw.model,
        token_usage: None,
        message_count,
        source_path: db_path.to_path_buf(),
    })
}

fn load_messages_from_db(db_path: &Path, composer_id: &str) -> Result<Vec<Message>, ProviderError> {
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let conn = open_readonly(db_path)?;
    if !table_exists(&conn, "cursorDiskKV")? {
        return Ok(Vec::new());
    }

    // 1. Read composer header to recover bubble order.
    let composer_key = format!("composerData:{composer_id}");
    let composer: Option<ComposerData> = conn
        .query_row(
            "SELECT value FROM cursorDiskKV WHERE key = ?1",
            [&composer_key],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let headers: Vec<HeaderEntry> = composer
        .and_then(|c| c.headers)
        .unwrap_or_default()
        .into_iter()
        .filter(|h| h.bubble_id.is_some())
        .collect();

    // 2. Read each bubble keyed under this composer. We collect both ways:
    //    headers give canonical ordering; a fallback LIKE scan catches bubbles
    //    not listed in the header (shouldn't happen in practice but matches
    //    the "tolerate corrupt" stance).
    let mut messages = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (idx, h) in headers.iter().enumerate() {
        let Some(bid) = h.bubble_id.as_deref() else {
            continue;
        };
        let key = format!("bubbleId:{composer_id}:{bid}");
        if let Some(bytes) = read_value(&conn, &key)? {
            if let Some(msg) = build_message(bid, h.bubble_type, &bytes, idx) {
                seen.insert(bid.to_string());
                messages.push(msg);
            }
        }
    }

    // Fallback scan: pick up any orphan bubbles. Sort by createdAt to
    // approximate the original ordering.
    let pattern = format!("bubbleId:{composer_id}:%");
    let mut stmt = conn
        .prepare("SELECT key, value FROM cursorDiskKV WHERE key LIKE ?1")
        .map_err(sql_err(db_path))?;
    let rows = stmt
        .query_map([&pattern], |row| {
            let key: String = row.get(0)?;
            let value: Vec<u8> = row.get(1)?;
            Ok((key, value))
        })
        .map_err(sql_err(db_path))?;
    let mut orphans: Vec<Message> = Vec::new();
    for row in rows.flatten() {
        let (key, value) = row;
        let prefix = format!("bubbleId:{composer_id}:");
        let Some(bid) = key.strip_prefix(&prefix) else {
            continue;
        };
        if seen.contains(bid) {
            continue;
        }
        if let Some(msg) = build_message(bid, None, &value, messages.len() + orphans.len()) {
            orphans.push(msg);
        }
    }
    orphans.sort_by_key(|m| m.timestamp);
    messages.extend(orphans);

    Ok(messages)
}

fn read_value(conn: &Connection, key: &str) -> Result<Option<Vec<u8>>, ProviderError> {
    match conn.query_row(
        "SELECT value FROM cursorDiskKV WHERE key = ?1",
        [key],
        |row| row.get::<_, Vec<u8>>(0),
    ) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(ProviderError::Parse {
            path: PathBuf::from(key),
            reason: e.to_string(),
        }),
    }
}

fn build_message(
    bubble_id: &str,
    header_type: Option<u8>,
    value: &[u8],
    idx: usize,
) -> Option<Message> {
    let raw: BubbleData = serde_json::from_slice(value).ok()?;

    let role = match raw.bubble_type.or(header_type)? {
        1 => Role::User,
        2 => Role::Assistant,
        _ => return None,
    };

    // Stable fallback when the bubble has no `createdAt`: anchor on the
    // Unix epoch + `idx` seconds. This is far in the past so it sorts
    // before any real session, but `idx` still preserves intra-session
    // ordering when the caller hands us the header position.
    let timestamp = raw
        .created_at
        .and_then(millis_to_datetime)
        .unwrap_or_else(|| {
            Utc.timestamp_opt(i64::try_from(idx).unwrap_or(0), 0)
                .single()
                .unwrap_or_else(Utc::now)
        });

    let mut content: Vec<ContentBlock> = Vec::new();

    if let Some(text) = raw.text.as_deref().filter(|s| !s.is_empty()) {
        content.extend(parse_text_with_code_blocks(text));
    } else if let Some(text) = raw.rich_text.as_deref().filter(|s| !s.is_empty()) {
        content.extend(parse_text_with_code_blocks(text));
    }

    for cb in &raw.code_blocks {
        let body = cb.code.as_deref().or(cb.content.as_deref()).unwrap_or("");
        if body.is_empty() {
            continue;
        }
        content.push(ContentBlock::CodeBlock {
            language: cb.language.clone(),
            code: body.to_string(),
        });
    }

    if let Some(tool) = &raw.tool_former {
        push_tool(tool, &mut content);
    }
    for tc in &raw.tool_calls {
        push_tool_v2(tc, &mut content);
    }

    Some(Message {
        id: MessageId(bubble_id.to_string()),
        role,
        timestamp,
        content,
        model: raw.model,
        token_usage: None,
    })
}

fn push_tool(tool: &ToolFormerData, content: &mut Vec<ContentBlock>) {
    let id = tool
        .tool_call_id
        .clone()
        .unwrap_or_else(|| String::from("cursor-tool"));
    let name = tool.name.clone().unwrap_or_else(|| String::from("tool"));
    let arguments = stringify_json(&tool.params);

    content.push(ContentBlock::ToolUse(ToolCall {
        id: id.clone(),
        name,
        arguments,
    }));

    if !tool.result.is_null() {
        content.push(ContentBlock::ToolResult(ToolResult {
            tool_call_id: id,
            success: tool
                .status
                .as_deref()
                .is_none_or(|s| !s.eq_ignore_ascii_case("error")),
            output: stringify_json(&tool.result),
        }));
    }
}

fn push_tool_v2(tc: &ToolCallData, content: &mut Vec<ContentBlock>) {
    let id = tc.id.clone().unwrap_or_else(|| String::from("cursor-tool"));
    let name = tc.name.clone().unwrap_or_else(|| String::from("tool"));
    let arguments = stringify_json(&tc.arguments);

    content.push(ContentBlock::ToolUse(ToolCall {
        id: id.clone(),
        name,
        arguments,
    }));

    if !tc.result.is_null() {
        content.push(ContentBlock::ToolResult(ToolResult {
            tool_call_id: id,
            success: tc
                .status
                .as_deref()
                .is_none_or(|s| !s.eq_ignore_ascii_case("error")),
            output: stringify_json(&tc.result),
        }));
    }
}

fn stringify_json(v: &serde_json::Value) -> String {
    if v.is_null() {
        return String::new();
    }
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    serde_json::to_string(v).unwrap_or_default()
}

fn open_readonly(db_path: &Path) -> Result<Connection, ProviderError> {
    Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(sql_err(db_path))
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, ProviderError> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
        [name],
        |_| Ok(()),
    )
    .map(|()| true)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(false),
        other => Err(ProviderError::Parse {
            path: PathBuf::new(),
            reason: other.to_string(),
        }),
    })
}

fn sql_err(path: &Path) -> impl Fn(rusqlite::Error) -> ProviderError + '_ {
    move |e: rusqlite::Error| ProviderError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    fn setup_db(dir: &Path) -> PathBuf {
        let db_path = state_db_path(dir);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value BLOB)",
            [],
        )
        .unwrap();
        db_path
    }

    fn insert(conn: &Connection, key: &str, json: &serde_json::Value) {
        let bytes = serde_json::to_vec(json).unwrap();
        conn.execute(
            "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, bytes],
        )
        .unwrap();
    }

    #[test]
    fn detect_returns_none_when_db_missing() {
        let tmp = TempDir::new().unwrap();
        let provider = CursorProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn discover_returns_empty_when_table_missing() {
        let tmp = TempDir::new().unwrap();
        let db_path = state_db_path(tmp.path());
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        Connection::open(&db_path).unwrap();
        let provider = CursorProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn parses_composer_and_bubbles() {
        let tmp = TempDir::new().unwrap();
        let db_path = setup_db(tmp.path());
        let conn = Connection::open(&db_path).unwrap();

        // 2026-01-01T00:00:00Z = 1767225600000 ms
        let composer = serde_json::json!({
            "composerId": "comp-1",
            "name": "Refactor auth",
            "createdAt": 1_767_225_600_000_i64,
            "lastUpdatedAt": 1_767_225_700_000_i64,
            "currentWorkspaceFolder": "/home/me/projects/myapp",
            "model": "claude-sonnet-4",
            "fullConversationHeadersOnly": [
                {"bubbleId": "b1", "type": 1},
                {"bubbleId": "b2", "type": 2},
            ],
        });
        insert(&conn, "composerData:comp-1", &composer);

        let user_bubble = serde_json::json!({
            "type": 1,
            "text": "How do I split this auth handler?",
            "createdAt": 1_767_225_600_000_i64,
        });
        insert(&conn, "bubbleId:comp-1:b1", &user_bubble);

        let assistant_bubble = serde_json::json!({
            "type": 2,
            "text": "Extract the token validation:",
            "createdAt": 1_767_225_650_000_i64,
            "codeBlocks": [
                {"languageId": "rust", "code": "fn validate(t: &str) -> bool { !t.is_empty() }"}
            ],
            "toolFormerData": {
                "toolCallId": "call-9",
                "name": "Edit",
                "params": {"file": "auth.rs"},
                "result": "Applied edit to auth.rs",
                "status": "success",
            },
        });
        insert(&conn, "bubbleId:comp-1:b2", &assistant_bubble);

        drop(conn);

        let provider = CursorProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.id.0, "comp-1");
        assert_eq!(s.provider, Provider::Cursor);
        assert_eq!(s.summary.as_deref(), Some("Refactor auth"));
        assert_eq!(s.project_name.as_deref(), Some("myapp"));
        assert_eq!(s.model.as_deref(), Some("claude-sonnet-4"));
        assert_eq!(s.message_count, 2);

        let messages = provider.load_messages(s).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);
        assert!(matches!(
            &messages[0].content[0],
            ContentBlock::Text(t) if t.contains("split this auth")
        ));
        assert_eq!(messages[1].role, Role::Assistant);
        let kinds: Vec<&'static str> = messages[1]
            .content
            .iter()
            .map(|c| match c {
                ContentBlock::Text(_) => "text",
                ContentBlock::CodeBlock { .. } => "code",
                ContentBlock::ToolUse(_) => "tool_use",
                ContentBlock::ToolResult(_) => "tool_result",
                ContentBlock::Thinking(_) => "thinking",
                ContentBlock::Error(_) => "error",
            })
            .collect();
        assert!(kinds.contains(&"text"));
        assert!(kinds.contains(&"code"));
        assert!(kinds.contains(&"tool_use"));
        assert!(kinds.contains(&"tool_result"));
    }

    #[test]
    fn corrupt_bubble_value_does_not_crash() {
        let tmp = TempDir::new().unwrap();
        let db_path = setup_db(tmp.path());
        let conn = Connection::open(&db_path).unwrap();

        let composer = serde_json::json!({
            "composerId": "comp-x",
            "createdAt": 1_767_225_600_000_i64,
            "fullConversationHeadersOnly": [
                {"bubbleId": "b1", "type": 1},
                {"bubbleId": "b2", "type": 2},
            ],
        });
        insert(&conn, "composerData:comp-x", &composer);
        // Bubble 1 valid, bubble 2 garbage.
        insert(
            &conn,
            "bubbleId:comp-x:b1",
            &serde_json::json!({"type": 1, "text": "ok"}),
        );
        conn.execute(
            "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
            rusqlite::params!["bubbleId:comp-x:b2", b"not-json"],
        )
        .unwrap();
        drop(conn);

        let provider = CursorProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        let messages = provider.load_messages(&sessions[0]).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id.0, "b1");
    }

    #[test]
    fn skips_composer_without_timestamp() {
        let tmp = TempDir::new().unwrap();
        let db_path = setup_db(tmp.path());
        let conn = Connection::open(&db_path).unwrap();

        // No createdAt or lastUpdatedAt — must be skipped, not crash.
        insert(
            &conn,
            "composerData:comp-no-ts",
            &serde_json::json!({"composerId": "comp-no-ts"}),
        );
        drop(conn);

        let provider = CursorProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert!(sessions.is_empty());
    }
}
