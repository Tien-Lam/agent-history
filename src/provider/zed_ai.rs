//! Zed AI (Zed editor's assistant panel) provider.
//!
//! Zed (<https://zed.dev>) ships an AI assistant panel that saves each
//! conversation as a JSON file under the editor's user-data directory.
//! On disk:
//!
//! - Linux:   `~/.local/share/zed/conversations/*.json` (also `~/.config/zed/conversations/`)
//! - macOS:   `~/Library/Application Support/Zed/conversations/*.json`
//! - Windows: `%APPDATA%\Zed\conversations\*.json`
//!
//! The on-disk schema has shifted across Zed releases — early builds used
//! a `buffer`+`anchor_range` shape, newer builds inline `text` per message.
//! This parser targets the inlined shape and tolerates field churn: unknown
//! roles, missing timestamps, and parse errors all skip rather than abort
//! discovery, matching the project-wide "corrupt session files are skipped,
//! never crash" stance.
//!
//! Expected JSON shape (fields are all optional unless noted):
//!
//! ```json
//! {
//!   "id": "uuid",
//!   "summary": "title text",
//!   "model": "anthropic/claude-sonnet-4",
//!   "workspace": "/abs/path/to/project",
//!   "created_at": "2026-01-01T00:00:00Z",
//!   "updated_at": "2026-01-01T00:05:00Z",
//!   "messages": [
//!     {
//!       "id": "msg-uuid",
//!       "role": "User" | "Assistant" | "System",
//!       "text": "message body",
//!       "timestamp": "2026-01-01T00:00:00Z"
//!     }
//!   ]
//! }
//! ```
//!
//! `created_at` / `updated_at` / `timestamp` accept either an RFC3339 string
//! or epoch milliseconds (`i64`) so we can absorb both common shapes.

use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

use super::{HistoryProvider, ProviderError};
use crate::model::{Message, MessageId, Provider, Role, Session, SessionId};
use crate::provider::claude_code::parse_text_with_code_blocks;

const CONVERSATIONS_SUBDIR: &str = "conversations";

pub struct ZedAiProvider {
    dirs: Vec<PathBuf>,
}

impl ZedAiProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| d.join(CONVERSATIONS_SUBDIR).exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

/// Zed base directories, ordered by likelihood. Each entry is the parent
/// directory that holds `conversations/`. Honors `AGHIST_HOME` (for tests)
/// and `ZED_HOME` as a power-user override.
fn base_dirs() -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = Vec::new();

    if let Ok(zed_home) = std::env::var("ZED_HOME") {
        result.push(PathBuf::from(zed_home));
    }

    if let Some(home) = super::home_dir() {
        // Linux (XDG data): ~/.local/share/zed
        result.push(home.join(".local").join("share").join("zed"));
        // Linux (XDG config): ~/.config/zed — older Zed builds wrote here
        result.push(home.join(".config").join("zed"));
        // macOS: ~/Library/Application Support/Zed
        result.push(home.join("Library").join("Application Support").join("Zed"));
        // Windows: %APPDATA%\Zed (mirrored under home for AGHIST_HOME tests)
        result.push(home.join("AppData").join("Roaming").join("Zed"));
    }

    if std::env::var("AGHIST_HOME").is_err() {
        if let Some(base) = directories::BaseDirs::new() {
            let appdata = base.config_dir().join("Zed");
            if !result.iter().any(|p| p == &appdata) {
                result.push(appdata);
            }
            let data = base.data_dir().join("Zed");
            if !result.iter().any(|p| p == &data) {
                result.push(data);
            }
        }
    }

    result
}

impl HistoryProvider for ZedAiProvider {
    fn provider(&self) -> Provider {
        Provider::ZedAi
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

        for base in &self.dirs {
            let conv_dir = base.join(CONVERSATIONS_SUBDIR);
            if !conv_dir.exists() {
                continue;
            }
            let entries = match std::fs::read_dir(&conv_dir) {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(path = %conv_dir.display(), error = %e, "skipping unreadable Zed conversations dir");
                    continue;
                }
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.extension().is_some_and(|x| x == "json") {
                    continue;
                }
                let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                if !seen.insert(canonical) {
                    continue;
                }
                match read_session(&path) {
                    Ok(Some(s)) => sessions.push(s),
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "skipping unreadable Zed conversation");
                    }
                }
            }
        }

        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        let bytes = match std::fs::read(&session.source_path) {
            Ok(b) => b,
            Err(e) => {
                return Err(ProviderError::Parse {
                    path: session.source_path.clone(),
                    reason: e.to_string(),
                });
            }
        };
        let raw: ZedConversation = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                return Err(ProviderError::Parse {
                    path: session.source_path.clone(),
                    reason: e.to_string(),
                });
            }
        };

        let messages = raw
            .messages
            .into_iter()
            .enumerate()
            .filter_map(|(idx, m)| build_message(m, idx))
            .collect();
        Ok(messages)
    }
}

#[derive(Debug, Deserialize)]
struct ZedConversation {
    id: Option<String>,
    summary: Option<String>,
    model: Option<String>,
    workspace: Option<String>,
    #[serde(default, alias = "createdAt")]
    created_at: Option<Timestamp>,
    #[serde(default, alias = "updatedAt")]
    updated_at: Option<Timestamp>,
    #[serde(default)]
    messages: Vec<ZedMessage>,
}

#[derive(Debug, Deserialize)]
struct ZedMessage {
    id: Option<String>,
    role: Option<String>,
    #[serde(default, alias = "content")]
    text: Option<String>,
    #[serde(default, alias = "createdAt")]
    timestamp: Option<Timestamp>,
    model: Option<String>,
}

/// Accepts either an RFC3339 string or epoch milliseconds. Older Zed builds
/// recorded millis; newer ones emit ISO strings.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Timestamp {
    Iso(String),
    Millis(i64),
}

impl Timestamp {
    fn to_utc(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Iso(s) => DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc)),
            Self::Millis(m) => Utc.timestamp_millis_opt(*m).single(),
        }
    }
}

fn read_session(path: &Path) -> Result<Option<Session>, ProviderError> {
    let bytes = std::fs::read(path).map_err(|e| ProviderError::Parse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    let raw: ZedConversation = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "failed to parse Zed conversation JSON");
            return Ok(None);
        }
    };

    // Recover an ID: explicit field → file stem → skip.
    let id = raw
        .id
        .clone()
        .or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        });
    let Some(id) = id else { return Ok(None) };

    // Started_at: created_at → earliest message timestamp → file mtime → skip.
    let started_at = raw
        .created_at
        .as_ref()
        .and_then(Timestamp::to_utc)
        .or_else(|| earliest_message_ts(&raw))
        .or_else(|| file_mtime(path));
    let Some(started_at) = started_at else {
        return Ok(None);
    };

    let ended_at = raw
        .updated_at
        .as_ref()
        .and_then(Timestamp::to_utc)
        .or_else(|| latest_message_ts(&raw));

    let project_path = raw.workspace.clone().map(PathBuf::from);
    let project_name = raw
        .workspace
        .as_deref()
        .and_then(super::project_name_from_path);

    let message_count = raw.messages.len();

    Ok(Some(Session {
        id: SessionId(id),
        provider: Provider::ZedAi,
        project_path,
        project_name,
        git_branch: None,
        started_at,
        ended_at,
        summary: raw.summary,
        model: raw.model,
        token_usage: None,
        message_count,
        source_path: path.to_path_buf(),
    }))
}

fn earliest_message_ts(conv: &ZedConversation) -> Option<DateTime<Utc>> {
    conv.messages
        .iter()
        .filter_map(|m| m.timestamp.as_ref().and_then(Timestamp::to_utc))
        .min()
}

fn latest_message_ts(conv: &ZedConversation) -> Option<DateTime<Utc>> {
    conv.messages
        .iter()
        .filter_map(|m| m.timestamp.as_ref().and_then(Timestamp::to_utc))
        .max()
}

fn file_mtime(path: &Path) -> Option<DateTime<Utc>> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let dur = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    let secs = i64::try_from(dur.as_secs()).ok()?;
    Utc.timestamp_opt(secs, dur.subsec_nanos()).single()
}

fn build_message(raw: ZedMessage, idx: usize) -> Option<Message> {
    let role = parse_role(raw.role.as_deref())?;
    let body = raw.text.unwrap_or_default();

    let timestamp = raw
        .timestamp
        .as_ref()
        .and_then(Timestamp::to_utc)
        .unwrap_or_else(|| {
            Utc.timestamp_opt(i64::try_from(idx).unwrap_or(0), 0)
                .single()
                .unwrap_or_else(Utc::now)
        });

    let id = raw.id.unwrap_or_else(|| format!("zed-msg-{idx}"));
    let content = if body.is_empty() {
        Vec::new()
    } else {
        parse_text_with_code_blocks(&body)
    };

    Some(Message {
        id: MessageId(id),
        role,
        timestamp,
        content,
        model: raw.model,
        token_usage: None,
    })
}

fn parse_role(role: Option<&str>) -> Option<Role> {
    match role?.to_ascii_lowercase().as_str() {
        "user" | "human" => Some(Role::User),
        "assistant" | "model" => Some(Role::Assistant),
        "system" => Some(Role::System),
        "tool" => Some(Role::Tool),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ContentBlock;
    use tempfile::TempDir;

    fn write_conv(dir: &Path, name: &str, json: &serde_json::Value) -> PathBuf {
        let conv_dir = dir.join(CONVERSATIONS_SUBDIR);
        std::fs::create_dir_all(&conv_dir).unwrap();
        let path = conv_dir.join(name);
        std::fs::write(&path, serde_json::to_vec_pretty(json).unwrap()).unwrap();
        path
    }

    #[test]
    fn detect_returns_none_when_dir_missing() {
        let tmp = TempDir::new().unwrap();
        let provider = ZedAiProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn parses_conversation_with_iso_timestamps() {
        let tmp = TempDir::new().unwrap();
        let json = serde_json::json!({
            "id": "conv-1",
            "summary": "Refactor auth",
            "model": "anthropic/claude-sonnet-4",
            "workspace": "/home/me/projects/myapp",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:05:00Z",
            "messages": [
                {
                    "id": "m1",
                    "role": "User",
                    "text": "How do I split this auth handler?",
                    "timestamp": "2026-01-01T00:00:00Z"
                },
                {
                    "id": "m2",
                    "role": "Assistant",
                    "text": "Extract token validation:\n```rust\nfn validate(t: &str) -> bool { !t.is_empty() }\n```",
                    "timestamp": "2026-01-01T00:01:00Z"
                }
            ]
        });
        write_conv(tmp.path(), "conv-1.json", &json);

        let provider = ZedAiProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.id.0, "conv-1");
        assert_eq!(s.provider, Provider::ZedAi);
        assert_eq!(s.summary.as_deref(), Some("Refactor auth"));
        assert_eq!(s.project_name.as_deref(), Some("myapp"));
        assert_eq!(s.model.as_deref(), Some("anthropic/claude-sonnet-4"));
        assert_eq!(s.message_count, 2);

        let messages = provider.load_messages(s).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);
        assert!(matches!(
            &messages[0].content[0],
            ContentBlock::Text(t) if t.contains("split this auth")
        ));
        assert_eq!(messages[1].role, Role::Assistant);
        // The fenced code block must be promoted to a CodeBlock content
        // block by parse_text_with_code_blocks.
        let has_code = messages[1].content.iter().any(|c| {
            matches!(c, ContentBlock::CodeBlock { language, .. } if language.as_deref() == Some("rust"))
        });
        assert!(has_code, "expected fenced rust block to surface as CodeBlock");
    }

    #[test]
    fn parses_conversation_with_millis_timestamps() {
        let tmp = TempDir::new().unwrap();
        // 2026-01-01T00:00:00Z = 1_767_225_600_000 ms
        let json = serde_json::json!({
            "id": "conv-ms",
            "summary": "Legacy build",
            "createdAt": 1_767_225_600_000_i64,
            "updatedAt": 1_767_225_700_000_i64,
            "messages": [
                {"role": "user", "text": "hi", "createdAt": 1_767_225_600_000_i64},
                {"role": "assistant", "content": "hello", "createdAt": 1_767_225_650_000_i64},
            ]
        });
        write_conv(tmp.path(), "conv-ms.json", &json);

        let provider = ZedAiProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.message_count, 2);
        let messages = provider.load_messages(s).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
        assert!(matches!(
            &messages[1].content[0],
            ContentBlock::Text(t) if t == "hello"
        ));
    }

    #[test]
    fn skips_corrupt_json_without_crash() {
        let tmp = TempDir::new().unwrap();
        let conv_dir = tmp.path().join(CONVERSATIONS_SUBDIR);
        std::fs::create_dir_all(&conv_dir).unwrap();
        std::fs::write(conv_dir.join("good.json"), serde_json::to_vec(&serde_json::json!({
            "id": "good",
            "summary": "ok",
            "created_at": "2026-01-01T00:00:00Z",
            "messages": [{"role": "user", "text": "hi", "timestamp": "2026-01-01T00:00:00Z"}],
        })).unwrap()).unwrap();
        std::fs::write(conv_dir.join("bad.json"), b"not-json{").unwrap();

        let provider = ZedAiProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id.0, "good");
    }

    #[test]
    fn skips_message_with_unknown_role() {
        let tmp = TempDir::new().unwrap();
        let json = serde_json::json!({
            "id": "conv-roles",
            "created_at": "2026-01-01T00:00:00Z",
            "messages": [
                {"role": "user", "text": "hi", "timestamp": "2026-01-01T00:00:00Z"},
                {"role": "narrator", "text": "ignored", "timestamp": "2026-01-01T00:00:30Z"},
                {"role": "assistant", "text": "yo", "timestamp": "2026-01-01T00:01:00Z"},
            ]
        });
        write_conv(tmp.path(), "conv.json", &json);

        let provider = ZedAiProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        let messages = provider.load_messages(&sessions[0]).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
    }

    #[test]
    fn falls_back_to_file_stem_when_id_missing() {
        let tmp = TempDir::new().unwrap();
        let json = serde_json::json!({
            "summary": "no id",
            "created_at": "2026-01-01T00:00:00Z",
            "messages": [],
        });
        write_conv(tmp.path(), "fallback-stem.json", &json);

        let provider = ZedAiProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id.0, "fallback-stem");
    }

    #[test]
    fn falls_back_to_file_mtime_when_no_timestamps() {
        let tmp = TempDir::new().unwrap();
        let json = serde_json::json!({
            "id": "no-ts",
            "messages": [],
        });
        write_conv(tmp.path(), "no-ts.json", &json);

        let provider = ZedAiProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        // Should not be skipped — mtime acts as the floor.
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id.0, "no-ts");
    }

    #[test]
    fn ignores_non_json_files() {
        let tmp = TempDir::new().unwrap();
        let conv_dir = tmp.path().join(CONVERSATIONS_SUBDIR);
        std::fs::create_dir_all(&conv_dir).unwrap();
        std::fs::write(conv_dir.join("notes.md"), "# notes").unwrap();
        std::fs::write(
            conv_dir.join("c.json"),
            serde_json::to_vec(&serde_json::json!({
                "id": "c",
                "created_at": "2026-01-01T00:00:00Z",
                "messages": [],
            }))
            .unwrap(),
        )
        .unwrap();

        let provider = ZedAiProvider::new(vec![tmp.path().to_path_buf()]);
        let sessions = provider.discover_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id.0, "c");
    }
}
