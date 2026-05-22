use super::*;
use std::path::{Path, PathBuf};

use crate::model::{ContentBlock, Role};
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
    assert!(
        has_code,
        "expected fenced rust block to surface as CodeBlock"
    );
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
fn tolerates_object_shaped_string_fields() {
    let tmp = TempDir::new().unwrap();
    let json = serde_json::json!({
        "id": {"id": "conv-object"},
        "summary": {"title": "Object summary"},
        "model": {"id": "zed-model"},
        "workspace": {"path": "/home/me/projects/objectapp"},
        "created_at": {"value": 1_767_225_600_000_i64},
        "messages": [
            {
                "id": {"id": "m1"},
                "role": {"role": "user"},
                "text": {"content": "object text"},
                "timestamp": {"timestamp": "2026-01-01T00:00:00Z"},
                "model": {"id": "message-model"}
            }
        ]
    });
    write_conv(tmp.path(), "conv-object.json", &json);

    let provider = ZedAiProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.id.0, "conv-object");
    assert_eq!(s.summary.as_deref(), Some("Object summary"));
    assert_eq!(s.project_name.as_deref(), Some("objectapp"));
    assert_eq!(s.model.as_deref(), Some("zed-model"));
    assert_eq!(s.started_at.to_rfc3339(), "2026-01-01T00:00:00+00:00");

    let messages = provider.load_messages(s).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id.0, "m1");
    assert_eq!(messages[0].role, Role::User);
    assert_eq!(messages[0].model.as_deref(), Some("message-model"));
    assert_eq!(
        messages[0].timestamp.to_rfc3339(),
        "2026-01-01T00:00:00+00:00"
    );
    assert!(matches!(
        &messages[0].content[0],
        ContentBlock::Text(text) if text == "object text"
    ));
}

#[test]
fn skips_corrupt_json_without_crash() {
    let tmp = TempDir::new().unwrap();
    let conv_dir = tmp.path().join(CONVERSATIONS_SUBDIR);
    std::fs::create_dir_all(&conv_dir).unwrap();
    std::fs::write(
        conv_dir.join("good.json"),
        serde_json::to_vec(&serde_json::json!({
            "id": "good",
            "summary": "ok",
            "created_at": "2026-01-01T00:00:00Z",
            "messages": [{"role": "user", "text": "hi", "timestamp": "2026-01-01T00:00:00Z"}],
        }))
        .unwrap(),
    )
    .unwrap();
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
            "not a zed message",
            {"role": "narrator", "text": "ignored", "timestamp": "2026-01-01T00:00:30Z"},
            {"role": "assistant", "text": "", "timestamp": "2026-01-01T00:01:00Z"},
        ]
    });
    write_conv(tmp.path(), "conv.json", &json);

    let provider = ZedAiProvider::new(vec![tmp.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    let load = provider.load_messages_with_stats(&sessions[0]).unwrap();
    assert_eq!(load.messages.len(), 2);
    assert_eq!(load.messages[0].role, Role::User);
    assert_eq!(load.messages[1].role, Role::Assistant);
    assert_eq!(load.parse_stats.records_seen, 4);
    assert_eq!(load.parse_stats.parse_errors, 1);
    assert_eq!(load.parse_stats.skipped_records, 1);
    assert_eq!(load.parse_stats.empty_content, 1);
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
