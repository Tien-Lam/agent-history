//! Cline (VS Code extension, formerly Claude Dev) provider.
//!
//! Cline (<https://github.com/cline/cline>) is a VS Code extension that stores
//! each task's conversation in a directory under VS Code's extension global
//! storage. On disk:
//!
//! - Linux:   `~/.config/Code/User/globalStorage/saoudrizwan.claude-dev/tasks/<ts>/`
//! - macOS:   `~/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/tasks/<ts>/`
//! - Windows: `%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\tasks\<ts>\`
//!
//! Cursor and Windsurf ship VS Code forks with the same extension; their
//! paths substitute `Cursor` or `Windsurf` for `Code`. We probe all three.
//!
//! Each task directory is named by a Unix timestamp in milliseconds (e.g.
//! `1698765432000`) and contains:
//!
//! - `api_conversation_history.json` — Anthropic Messages API array (canonical)
//! - `ui_messages.json` — UI-layer messages (we use the first entry's `text`
//!   as the session summary)
//! - `task_metadata.json` (newer Cline): `{createdAt, updatedAt, ...}` timestamps
//!
//! `api_conversation_history.json` follows the Anthropic Messages API shape:
//!
//! ```json
//! [
//!   {"role": "user",      "content": [{"type": "text", "text": "..."}]},
//!   {"role": "assistant", "content": [{"type": "text", "text": "..."}]},
//!   ...
//! ]
//! ```
//!
//! Content blocks may also include `tool_use` / `tool_result` entries.
//! Unknown block types and unknown roles are silently skipped.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use super::{HistoryProvider, ProviderError};
use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, ToolCall, ToolResult,
};
use crate::provider::claude_code::parse_text_with_code_blocks;

const EXTENSION_ID: &str = "saoudrizwan.claude-dev";
const TASKS_SUBDIR: &str = "tasks";
const API_HISTORY_FILE: &str = "api_conversation_history.json";
const UI_MESSAGES_FILE: &str = "ui_messages.json";
const METADATA_FILE: &str = "task_metadata.json";

pub struct ClineProvider {
    dirs: Vec<PathBuf>,
}

impl ClineProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| tasks_dir(d).exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

fn tasks_dir(base: &Path) -> PathBuf {
    base.join(EXTENSION_ID).join(TASKS_SUBDIR)
}

/// VS Code (and fork) global-storage directories, each being the parent that
/// holds `saoudrizwan.claude-dev/tasks/`. We check VS Code, Cursor, and
/// Windsurf. Honors `CLINE_HOME` for testability.
fn base_dirs() -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = Vec::new();

    if let Ok(cline_home) = std::env::var("CLINE_HOME") {
        result.push(PathBuf::from(cline_home));
        return result;
    }

    if let Some(home) = super::home_dir() {
        for editor in &["Code", "Cursor", "Windsurf"] {
            result.push(
                home.join(".config")
                    .join(editor)
                    .join("User")
                    .join("globalStorage"),
            );
            result.push(
                home.join("Library")
                    .join("Application Support")
                    .join(editor)
                    .join("User")
                    .join("globalStorage"),
            );
            result.push(
                home.join("AppData")
                    .join("Roaming")
                    .join(editor)
                    .join("User")
                    .join("globalStorage"),
            );
        }
    }

    result
}

// ── Serde shapes ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ApiMessage {
    role: String,
    #[serde(default)]
    content: ApiContent,
}

#[derive(Deserialize, Default)]
#[serde(untagged)]
enum ApiContent {
    Blocks(Vec<ApiBlock>),
    Text(String),
    #[default]
    Empty,
}

#[derive(Deserialize)]
struct ApiBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
    // tool_use
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<serde_json::Value>,
    // tool_result
    #[serde(default)]
    tool_use_id: Option<String>,
    #[serde(default)]
    content: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct UiMessage {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct TaskMetadata {
    #[serde(rename = "createdAt", default)]
    created_at: Option<i64>,
}

// ── Provider impl ─────────────────────────────────────────────────────────────

impl HistoryProvider for ClineProvider {
    fn provider(&self) -> Provider {
        Provider::Cline
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

        for base in &self.dirs {
            let td = tasks_dir(base);
            if !td.is_dir() {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&td) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                if !seen.insert(canonical) {
                    continue;
                }
                if let Some(session) = parse_task_dir(&path) {
                    sessions.push(session);
                }
            }
        }

        sessions.sort_by_key(|s| Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        let history_path = session.source_path.join(API_HISTORY_FILE);
        parse_api_history(&history_path, &session.started_at).map_err(|reason| {
            ProviderError::Parse {
                path: history_path.clone(),
                reason,
            }
        })
    }
}

// ── Parsing helpers ───────────────────────────────────────────────────────────

fn parse_task_dir(path: &Path) -> Option<Session> {
    let task_id = path.file_name()?.to_str()?.to_string();

    let history_path = path.join(API_HISTORY_FILE);
    if !history_path.exists() {
        return None;
    }

    let started_at = started_at_for(path, &task_id);
    let summary = task_summary(path);

    Some(Session {
        id: SessionId(task_id),
        provider: Provider::Cline,
        project_path: None,
        project_name: None,
        git_branch: None,
        started_at,
        ended_at: None,
        summary,
        model: None,
        token_usage: None,
        message_count: 0,
        source_path: path.to_path_buf(),
    })
}

/// Parse the session start timestamp. Priority:
/// 1. `task_metadata.json` `createdAt` (ms since epoch)
/// 2. Task directory name if it looks like an ms-epoch integer
/// 3. Directory mtime
fn started_at_for(path: &Path, task_id: &str) -> DateTime<Utc> {
    let meta_path = path.join(METADATA_FILE);
    if let Ok(bytes) = std::fs::read(&meta_path) {
        if let Ok(meta) = serde_json::from_slice::<TaskMetadata>(&bytes) {
            if let Some(ms) = meta.created_at {
                let secs = ms / 1000;
                let nsecs = u32::try_from((ms % 1000) * 1_000_000).unwrap_or(0);
                if let Some(dt) = Utc.timestamp_opt(secs, nsecs).single() {
                    return dt;
                }
            }
        }
    }

    if let Ok(ms) = task_id.parse::<i64>() {
        let secs = ms / 1000;
        let nsecs = u32::try_from((ms % 1000) * 1_000_000).unwrap_or(0);
        if let Some(dt) = Utc.timestamp_opt(secs, nsecs).single() {
            return dt;
        }
    }

    path.metadata()
        .and_then(|m| m.modified())
        .map_or_else(|_| Utc::now(), DateTime::<Utc>::from)
}

/// Extract a human-readable summary from `ui_messages.json` first entry's text.
fn task_summary(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path.join(UI_MESSAGES_FILE)).ok()?;
    let msgs: Vec<UiMessage> = serde_json::from_slice(&bytes).ok()?;
    let text = msgs.into_iter().find_map(|m| m.text)?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(if text.chars().count() > 120 {
        format!("{}…", text.chars().take(119).collect::<String>())
    } else {
        text
    })
}

fn parse_api_history(path: &Path, base_ts: &DateTime<Utc>) -> Result<Vec<Message>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let raw: Vec<ApiMessage> =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse: {e}"))?;

    let mut messages = Vec::with_capacity(raw.len());
    for (idx, msg) in raw.into_iter().enumerate() {
        let role = match msg.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => continue,
        };

        let blocks = api_content_to_blocks(msg.content);
        if blocks.is_empty() {
            continue;
        }

        // Spread messages 1 ms apart so ordering is stable even without embedded timestamps
        let timestamp = *base_ts + chrono::Duration::milliseconds(i64::try_from(idx).unwrap_or(0));

        messages.push(Message {
            id: MessageId(format!("msg-{idx}")),
            role,
            timestamp,
            content: blocks,
            model: None,
            token_usage: None,
        });
    }

    Ok(messages)
}

fn api_content_to_blocks(content: ApiContent) -> Vec<ContentBlock> {
    match content {
        ApiContent::Text(t) if !t.trim().is_empty() => parse_text_with_code_blocks(&t),
        ApiContent::Blocks(blocks) => blocks.into_iter().flat_map(api_block_to_content).collect(),
        _ => vec![],
    }
}

fn api_block_to_content(block: ApiBlock) -> Vec<ContentBlock> {
    match block.kind.as_str() {
        "text" => {
            let t = block.text.unwrap_or_default();
            if t.trim().is_empty() {
                vec![]
            } else {
                parse_text_with_code_blocks(&t)
            }
        }
        "tool_use" => {
            let id = block.id.unwrap_or_default();
            let name = block.name.unwrap_or_default();
            let arguments = block
                .input
                .map(|v| {
                    if let serde_json::Value::String(s) = v {
                        s
                    } else {
                        serde_json::to_string_pretty(&v).unwrap_or_default()
                    }
                })
                .unwrap_or_default();
            vec![ContentBlock::ToolUse(ToolCall { id, name, arguments })]
        }
        "tool_result" => {
            let tool_call_id = block.tool_use_id.unwrap_or_default();
            let output = block
                .content
                .map(|v| match v {
                    serde_json::Value::String(s) => s,
                    serde_json::Value::Array(arr) => arr
                        .into_iter()
                        .filter_map(|b| {
                            if b.get("type")?.as_str()? == "text" {
                                b.get("text")?.as_str().map(str::to_string)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    other => serde_json::to_string_pretty(&other).unwrap_or_default(),
                })
                .unwrap_or_default();
            vec![ContentBlock::ToolResult(ToolResult {
                tool_call_id,
                success: true,
                output,
            })]
        }
        _ => vec![],
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    fn make_task(root: &Path, task_id: &str, api_json: &str) -> PathBuf {
        let task = root.join(EXTENSION_ID).join(TASKS_SUBDIR).join(task_id);
        fs::create_dir_all(&task).unwrap();
        write_file(&task, API_HISTORY_FILE, api_json);
        task
    }

    fn provider_for(tmp: &TempDir) -> ClineProvider {
        ClineProvider::new(vec![tmp.path().to_path_buf()])
    }

    #[test]
    fn detect_returns_none_when_dir_missing() {
        let tmp = TempDir::new().unwrap();
        let p = ClineProvider::new(vec![tmp.path().to_path_buf()]);
        assert!(p.discover_sessions().unwrap().is_empty());
    }

    #[test]
    fn discovers_session_from_task_dir() {
        let tmp = TempDir::new().unwrap();
        make_task(tmp.path(), "1698765432000", "[]");
        let sessions = provider_for(&tmp).discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id.0, "1698765432000");
    }

    #[test]
    fn parses_timestamp_from_task_dir_name() {
        let tmp = TempDir::new().unwrap();
        make_task(tmp.path(), "1698765432000", "[]");
        let sessions = provider_for(&tmp).discover_sessions().unwrap();
        assert_eq!(sessions[0].started_at.timestamp(), 1_698_765_432);
    }

    #[test]
    fn uses_task_metadata_created_at_over_dir_name() {
        let tmp = TempDir::new().unwrap();
        let task_path = tmp.path().join(EXTENSION_ID).join(TASKS_SUBDIR).join("1698765432000");
        fs::create_dir_all(&task_path).unwrap();
        write_file(&task_path, API_HISTORY_FILE, "[]");
        write_file(&task_path, METADATA_FILE, "{\"createdAt\":1700000000000}");
        let sessions = provider_for(&tmp).discover_sessions().unwrap();
        assert_eq!(sessions[0].started_at.timestamp(), 1_700_000_000);
    }

    #[test]
    fn extracts_summary_from_ui_messages() {
        let tmp = TempDir::new().unwrap();
        let task_path = tmp.path().join(EXTENSION_ID).join(TASKS_SUBDIR).join("123");
        fs::create_dir_all(&task_path).unwrap();
        write_file(&task_path, API_HISTORY_FILE, "[]");
        write_file(
            &task_path,
            UI_MESSAGES_FILE,
            "[{\"type\":\"say\",\"say\":\"task\",\"text\":\"Implement feature X\"}]",
        );
        let sessions = provider_for(&tmp).discover_sessions().unwrap();
        assert_eq!(sessions[0].summary.as_deref(), Some("Implement feature X"));
    }

    #[test]
    fn skips_unknown_roles() {
        let tmp = TempDir::new().unwrap();
        let api = r#"[
            {"role":"system","content":[{"type":"text","text":"sys"}]},
            {"role":"user","content":[{"type":"text","text":"hello"}]}
        ]"#;
        make_task(tmp.path(), "1698765432000", api);
        let p = provider_for(&tmp);
        let sessions = p.discover_sessions().unwrap();
        let msgs = p.load_messages(&sessions[0]).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::User);
    }

    #[test]
    fn parses_tool_use_and_tool_result_blocks() {
        let tmp = TempDir::new().unwrap();
        let api = r#"[
            {"role":"assistant","content":[
                {"type":"tool_use","id":"tu1","name":"write_file","input":{"path":"x.rs"}}
            ]},
            {"role":"user","content":[
                {"type":"tool_result","tool_use_id":"tu1","content":"ok"}
            ]}
        ]"#;
        make_task(tmp.path(), "1698765432000", api);
        let p = provider_for(&tmp);
        let sessions = p.discover_sessions().unwrap();
        let msgs = p.load_messages(&sessions[0]).unwrap();
        assert_eq!(msgs.len(), 2);
        assert!(
            matches!(&msgs[0].content[0], ContentBlock::ToolUse(tc) if tc.name == "write_file")
        );
        assert!(
            matches!(&msgs[1].content[0], ContentBlock::ToolResult(tr) if tr.tool_call_id == "tu1")
        );
    }

    #[test]
    fn load_messages_errors_on_corrupt_json() {
        let tmp = TempDir::new().unwrap();
        make_task(tmp.path(), "1698765432000", "not json");
        let p = provider_for(&tmp);
        let sessions = p.discover_sessions().unwrap();
        assert!(p.load_messages(&sessions[0]).is_err());
    }

    #[test]
    fn skips_directory_without_api_history() {
        let tmp = TempDir::new().unwrap();
        let task = tmp.path().join(EXTENSION_ID).join(TASKS_SUBDIR).join("1698765432000");
        fs::create_dir_all(&task).unwrap();
        let sessions = provider_for(&tmp).discover_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn string_content_parsed_as_text() {
        let tmp = TempDir::new().unwrap();
        let api = r#"[{"role":"user","content":"plain string content"}]"#;
        make_task(tmp.path(), "1698765432000", api);
        let p = provider_for(&tmp);
        let sessions = p.discover_sessions().unwrap();
        let msgs = p.load_messages(&sessions[0]).unwrap();
        assert_eq!(msgs.len(), 1);
        assert!(matches!(&msgs[0].content[0], ContentBlock::Text(_)));
    }
}
