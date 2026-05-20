use aghist::model::{ContentBlock, Provider, Role};
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::HistoryProvider;

use super::common;
use super::common::helpers::fixtures_dir;

#[test]
fn claude_discover_sessions() {
    let provider = ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.id.0, "session-abc123");
    assert_eq!(s.provider, Provider::ClaudeCode);
    assert_eq!(s.project_name.as_deref(), Some("test-project"));
    assert_eq!(s.git_branch.as_deref(), Some("main"));
    assert_eq!(s.summary.as_deref(), Some("Fix the build error"));
    assert_eq!(s.model.as_deref(), Some("claude-sonnet-4-20250514"));
    assert_eq!(s.message_count, 4);

    let usage = s.token_usage.as_ref().unwrap();
    assert_eq!(usage.input_tokens, 300);
    assert_eq!(usage.output_tokens, 150);
}

#[test]
fn claude_load_messages() {
    let provider = ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 4);

    assert_eq!(messages[0].role, Role::User);
    assert_eq!(messages[0].id.0, "msg-001");
    assert!(
        matches!(&messages[0].content[0], ContentBlock::Text(t) if t.contains("Fix the build"))
    );

    assert_eq!(messages[1].role, Role::Assistant);
    assert!(
        matches!(&messages[1].content[0], ContentBlock::Text(t) if t.contains("fix the build"))
    );
    assert!(matches!(&messages[1].content[1], ContentBlock::ToolUse(tc) if tc.name == "Read"));

    assert_eq!(messages[2].role, Role::User);
    assert!(
        matches!(&messages[2].content[0], ContentBlock::ToolResult(tr) if tr.success && tr.output.contains("fn main"))
    );

    assert_eq!(messages[3].role, Role::Assistant);
    let has_thinking = messages[3]
        .content
        .iter()
        .any(|c| matches!(c, ContentBlock::Thinking(_)));
    let has_code = messages[3]
        .content
        .iter()
        .any(|c| matches!(c, ContentBlock::CodeBlock { .. }));
    assert!(has_thinking, "expected thinking block");
    assert!(has_code, "expected code block from markdown");
}

#[test]
fn claude_tool_result_array_content_joins_text_parts() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("claude-tool-array")
        .raw_line(
            r#"{"type":"user","uuid":"msg-tool-result","timestamp":"2025-01-01T00:00:00Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-001","content":[{"type":"text","text":"first line"},{"type":"image","text":"ignored"},{"type":"text","text":"second line"}]}]}}"#,
        )
        .done()
        .build();
    let provider = ClaudeCodeProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 1);
    assert!(
        matches!(&messages[0].content[0], ContentBlock::ToolResult(tr) if tr.tool_call_id == "tool-001" && tr.output == "first line\nsecond line")
    );
}
