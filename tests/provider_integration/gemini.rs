use aghist::model::{ContentBlock, Provider, Role};
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::HistoryProvider;

use super::common;
use super::common::helpers::fixtures_dir;

#[test]
fn gemini_discover_sessions() {
    let provider = GeminiCliProvider::new(vec![fixtures_dir().join("gemini")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.id.0, "gemini-sess-001");
    assert_eq!(s.provider, Provider::GeminiCli);
    assert_eq!(s.project_name.as_deref(), Some("test-project"));
    assert_eq!(s.model.as_deref(), Some("gemini-2.5-pro"));
    assert_eq!(s.message_count, 2);
    assert!(s.summary.as_ref().unwrap().contains("async/await"));

    let usage = s.token_usage.as_ref().unwrap();
    assert_eq!(usage.input_tokens, 60);
    assert_eq!(usage.output_tokens, 150);
}

#[test]
fn gemini_load_messages() {
    let provider = GeminiCliProvider::new(vec![fixtures_dir().join("gemini")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 2);

    assert_eq!(messages[0].role, Role::User);
    assert!(matches!(&messages[0].content[0], ContentBlock::Text(t) if t.contains("async/await")));

    assert_eq!(messages[1].role, Role::Assistant);
    let has_code = messages[1]
        .content
        .iter()
        .any(|c| matches!(c, ContentBlock::CodeBlock { .. }));
    let has_thinking = messages[1]
        .content
        .iter()
        .any(|c| matches!(c, ContentBlock::Thinking(_)));
    let has_tool = messages[1]
        .content
        .iter()
        .any(|c| matches!(c, ContentBlock::ToolUse(_)));
    assert!(has_code, "expected code block");
    assert!(has_thinking, "expected thinking block");
    assert!(has_tool, "expected tool call");
}

#[test]
fn gemini_tool_response_keeps_freeform_json() {
    let fixture = common::fixtures::gemini::GeminiFixtureBuilder::new()
        .add_session("gemini-tool-response")
        .raw_message(
            r#"{"id":"gm-tool","timestamp":"2025-01-01T00:00:00Z","type":"gemini","content":"running tool","toolCalls":[{"id":"tool-1","name":"inspect","args":{"path":"src/main.rs"},"response":{"nested":{"answer":42}}}],"model":"gemini-2.5-pro"}"#,
        )
        .done()
        .build();
    let provider = GeminiCliProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 1);
    let output = messages[0].content.iter().find_map(|block| match block {
        ContentBlock::ToolResult(result) => Some(result.output.as_str()),
        _ => None,
    });
    let output = output.expect("expected tool result output");
    assert!(output.contains("\"nested\""));
    assert!(output.contains("\"answer\": 42"));
}
