use aghist::model::{ContentBlock, Provider, Role};
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;
use std::fs;

use super::common::helpers::fixtures_dir;

#[test]
fn opencode_discover_sessions() {
    let provider = OpenCodeProvider::new(vec![fixtures_dir().join("opencode")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.id.0, "sess-001");
    assert_eq!(s.provider, Provider::OpenCode);
    assert_eq!(s.project_name.as_deref(), Some("dbproject"));
    assert_eq!(s.summary.as_deref(), Some("Refactor database layer"));
    assert_eq!(s.message_count, 2);
    assert_eq!(
        s.source_path.file_name().and_then(|name| name.to_str()),
        Some("session-001.json")
    );
}

#[test]
fn opencode_load_messages() {
    let provider = OpenCodeProvider::new(vec![fixtures_dir().join("opencode")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 2);

    assert_eq!(messages[0].role, Role::User);
    assert!(
        matches!(&messages[0].content[0], ContentBlock::Text(t) if t.contains("connection pool"))
    );

    assert_eq!(messages[1].role, Role::Assistant);
    let has_text = messages[1]
        .content
        .iter()
        .any(|c| matches!(c, ContentBlock::Text(_)));
    let has_diff = messages[1].content.iter().any(|c| {
        matches!(c, ContentBlock::CodeBlock { language, .. } if language.as_deref().unwrap_or("").contains("diff"))
    });
    assert!(has_text, "expected text block");
    assert!(has_diff, "expected diff code block");
}

#[test]
fn opencode_tolerates_object_content_and_tool_output() {
    let fixture = super::common::fixtures::opencode::OpenCodeFixtureBuilder::new()
        .add_session("oc-shape-drift")
        .raw_message(
            "msg-shape",
            r#"{"id":"msg-shape","role":"assistant","time":{"created":1735689600000},"summary":{"title":{"text":"fallback title"}}}"#,
        )
        .done()
        .build();
    let part_dir = fixture.base_path.join("part").join("msg-shape");
    fs::create_dir_all(&part_dir).unwrap();
    fs::write(
        part_dir.join("part-001.json"),
        r#"{"type":"text","text":{"content":"object part"}}"#,
    )
    .unwrap();
    fs::write(
        part_dir.join("part-002.json"),
        r#"{"type":"tool","tool":{"name":"Read"},"callID":"call-1","state":{"status":"completed","input":{"path":"src/lib.rs"},"output":{"stdout":["line one","line two"]}}}"#,
    )
    .unwrap();

    let provider = OpenCodeProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 1);
    assert!(messages[0]
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Text(text) if text == "object part")));
    assert!(
        messages[0]
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolResult(result) if result.tool_call_id == "call-1" && result.output.contains("line one")))
    );
}

#[test]
fn opencode_load_messages_with_stats_reports_malformed_skipped_and_empty_files() {
    let fixture = super::common::fixtures::opencode::OpenCodeFixtureBuilder::new()
        .add_session("oc-malformed")
        .raw_message("msg-bad-json", "not-json")
        .raw_message(
            "msg-unknown-role",
            r#"{"id":"msg-unknown-role","role":"system","timestamp":"2025-01-01T00:00:01Z","content":"skip"}"#,
        )
        .raw_message(
            "msg-empty",
            r#"{"id":"msg-empty","role":"assistant","timestamp":"2025-01-01T00:00:02Z","content":""}"#,
        )
        .user("kept")
        .done()
        .build();

    let provider = OpenCodeProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    let load = provider.load_messages_with_stats(&sessions[0]).unwrap();

    assert_eq!(load.messages.len(), 1);
    assert_eq!(load.messages[0].role, Role::User);
    assert_eq!(load.parse_stats.records_seen, 4);
    assert_eq!(load.parse_stats.parse_errors, 1);
    assert_eq!(load.parse_stats.skipped_records, 1);
    assert_eq!(load.parse_stats.empty_content, 1);
}

#[test]
fn opencode_tolerates_object_metadata_fields() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let session_dir = base.join("session").join("proj-object");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("oc-object.json"),
        r#"{
            "id":{"id":"oc-object"},
            "title":{"text":"Object OpenCode session"},
            "directory":{"path":"/home/me/projects/ocapp"},
            "time":{"created":{"value":1735689600000},"updated":{"value":1735689660000}},
            "model":{"modelID":{"id":"opencode-model"}}
        }"#,
    )
    .unwrap();

    let message_dir = base.join("message").join("oc-object");
    fs::create_dir_all(&message_dir).unwrap();
    fs::write(
        message_dir.join("msg-object.json"),
        r#"{
            "id":{"id":"msg-object"},
            "role":{"role":"assistant"},
            "time":{"created":{"value":1735689601000}},
            "summary":{"title":{"text":"fallback title"}},
            "tokens":{"input":"4","output":{"value":5},"cache":{"read":{"tokens":1},"write":"2"}},
            "model":{"modelID":{"id":"message-model"}}
        }"#,
    )
    .unwrap();

    let part_dir = base.join("part").join("msg-object");
    fs::create_dir_all(&part_dir).unwrap();
    fs::write(
        part_dir.join("part-001.json"),
        r#"{"type":{"type":"text"},"text":{"content":"object metadata part"}}"#,
    )
    .unwrap();
    fs::write(
        part_dir.join("part-002.json"),
        r#"{"type":{"type":"tool"},"tool":{"name":"Read"},"callID":{"id":"call-object"},"state":{"status":{"state":"completed"},"input":{"path":"src/lib.rs"},"output":{"content":"tool ok"}}}"#,
    )
    .unwrap();

    let provider = OpenCodeProvider::new(vec![base.to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.id.0, "oc-object");
    assert_eq!(session.summary.as_deref(), Some("Object OpenCode session"));
    assert_eq!(session.project_name.as_deref(), Some("ocapp"));
    assert_eq!(session.model.as_deref(), Some("opencode-model"));

    let messages = provider.load_messages(session).unwrap();
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.id.0, "msg-object");
    assert_eq!(message.role, Role::Assistant);
    assert_eq!(message.model.as_deref(), Some("message-model"));
    let usage = message.token_usage.as_ref().unwrap();
    assert_eq!(usage.input_tokens, 4);
    assert_eq!(usage.output_tokens, 5);
    assert_eq!(usage.cache_read_tokens, Some(1));
    assert_eq!(usage.cache_write_tokens, Some(2));
    assert!(message
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Text(text) if text == "object metadata part")));
    assert!(
        message
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolResult(result) if result.tool_call_id == "call-object" && result.success && result.output == "tool ok"))
    );
}
