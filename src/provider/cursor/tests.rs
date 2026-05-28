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
fn tolerates_object_shaped_string_fields() {
    let tmp = TempDir::new().unwrap();
    let db_path = setup_db(tmp.path());
    let conn = Connection::open(&db_path).unwrap();

    let composer = serde_json::json!({
        "composerId": {"id": "comp-object"},
        "name": {"title": "Object Cursor chat"},
        "createdAt": "1767225600000",
        "lastUpdatedAt": {"value": 1_767_225_700_000_i64},
        "currentWorkspaceFolder": {"path": "/home/me/projects/cursorapp"},
        "model": {"id": "cursor-model"},
        "fullConversationHeadersOnly": [
            {"bubbleId": {"id": "b-object"}, "type": "2"},
            "skip invalid header"
        ],
    });
    insert(&conn, "composerData:comp-object", &composer);

    let bubble = serde_json::json!({
        "type": {"value": 2},
        "text": {"content": "object cursor text"},
        "createdAt": "1767225600000",
        "model": {"id": "bubble-model"},
        "codeBlocks": [
            {"languageId": {"id": "rust"}, "code": {"text": "fn cursor() {}"}},
            "skip invalid code block"
        ],
        "toolCalls": [
            {
                "id": {"id": "tool-object"},
                "name": {"name": "Read"},
                "arguments": {"path": "src/lib.rs"},
                "result": {"content": "read ok"},
                "status": {"state": "success"}
            },
            "skip invalid tool call"
        ],
    });
    insert(&conn, "bubbleId:comp-object:b-object", &bubble);
    drop(conn);

    let provider = CursorProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.id.0, "comp-object");
    assert_eq!(session.summary.as_deref(), Some("Object Cursor chat"));
    assert_eq!(session.project_name.as_deref(), Some("cursorapp"));
    assert_eq!(session.model.as_deref(), Some("cursor-model"));
    assert_eq!(session.message_count, 1);

    let messages = provider.load_messages(session).unwrap();
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.id.0, "b-object");
    assert_eq!(message.role, Role::Assistant);
    assert_eq!(message.model.as_deref(), Some("bubble-model"));
    assert!(matches!(
        &message.content[0],
        ContentBlock::Text(text) if text == "object cursor text"
    ));
    assert!(message.content.iter().any(|block| {
        matches!(
            block,
            ContentBlock::CodeBlock { language, code }
                if language.as_deref() == Some("rust") && code == "fn cursor() {}"
        )
    }));
    assert!(message
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolUse(tool) if tool.name == "Read")));
    assert!(
        message
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolResult(result) if result.tool_call_id == "tool-object"))
    );
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
            {"bubbleId": "b3", "type": 99},
            {"bubbleId": "b4", "type": 2},
        ],
    });
    insert(&conn, "composerData:comp-x", &composer);
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
    insert(
        &conn,
        "bubbleId:comp-x:b3",
        &serde_json::json!({"type": 99, "text": "skip me"}),
    );
    insert(&conn, "bubbleId:comp-x:b4", &serde_json::json!({"type": 2}));
    drop(conn);

    let provider = CursorProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    let load = provider.load_messages_with_stats(&sessions[0]).unwrap();

    assert_eq!(load.messages.len(), 2);
    assert_eq!(load.messages[0].id.0, "b1");
    assert_eq!(load.messages[1].id.0, "b4");
    assert_eq!(load.parse_stats.records_seen, 4);
    assert_eq!(load.parse_stats.parse_errors, 1);
    assert_eq!(load.parse_stats.skipped_records, 1);
    assert_eq!(load.parse_stats.empty_content, 1);
}

#[test]
fn skips_oversized_composer_rows() {
    let tmp = TempDir::new().unwrap();
    let db_path = setup_db(tmp.path());
    let conn = Connection::open(&db_path).unwrap();
    let oversized = vec![b' '; usize::try_from(store::MAX_CURSOR_VALUE_BYTES).unwrap() + 1];
    conn.execute(
        "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
        rusqlite::params!["composerData:too-big", oversized],
    )
    .unwrap();
    drop(conn);

    let provider = CursorProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();

    assert!(sessions.is_empty());
}

#[test]
fn skips_oversized_bubble_rows() {
    let tmp = TempDir::new().unwrap();
    let db_path = setup_db(tmp.path());
    let conn = Connection::open(&db_path).unwrap();

    insert(
        &conn,
        "composerData:comp-huge-bubble",
        &serde_json::json!({
            "composerId": "comp-huge-bubble",
            "createdAt": 1_767_225_600_000_i64,
            "fullConversationHeadersOnly": [
                {"bubbleId": "b1", "type": 1},
                {"bubbleId": "b2", "type": 2},
            ],
        }),
    );
    let oversized = vec![b' '; usize::try_from(store::MAX_CURSOR_VALUE_BYTES).unwrap() + 1];
    conn.execute(
        "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
        rusqlite::params!["bubbleId:comp-huge-bubble:b1", oversized],
    )
    .unwrap();
    insert(
        &conn,
        "bubbleId:comp-huge-bubble:b2",
        &serde_json::json!({"type": 2, "text": "kept"}),
    );
    drop(conn);

    let provider = CursorProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    let load = provider.load_messages_with_stats(&sessions[0]).unwrap();

    assert_eq!(load.messages.len(), 1);
    assert_eq!(load.messages[0].id.0, "b2");
    assert_eq!(load.parse_stats.records_seen, 1);
}

#[test]
fn orphan_bubble_row_decode_error_is_reported() {
    let tmp = TempDir::new().unwrap();
    let db_path = setup_db(tmp.path());
    let conn = Connection::open(&db_path).unwrap();

    insert(
        &conn,
        "composerData:comp-row-error",
        &serde_json::json!({
            "composerId": "comp-row-error",
            "createdAt": 1_767_225_600_000_i64,
        }),
    );
    conn.execute(
        "INSERT INTO cursorDiskKV (key, value) VALUES (?1, ?2)",
        rusqlite::params!["bubbleId:comp-row-error:b1", 42_i64],
    )
    .unwrap();
    drop(conn);

    let provider = CursorProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    let err = provider.load_messages_with_stats(&sessions[0]).unwrap_err();

    assert!(matches!(
        err,
        ProviderError::Parse { reason, .. } if reason.contains("Invalid column type")
    ));
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
