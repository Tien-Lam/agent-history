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

use std::path::PathBuf;

use super::{HistoryProvider, ProviderError};
use crate::model::{Message, Provider, Session};
use crate::provider::cursor_store::{load_messages_from_db, read_sessions, state_db_path};

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    use crate::model::{ContentBlock, Role};
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
