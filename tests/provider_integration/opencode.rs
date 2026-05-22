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
