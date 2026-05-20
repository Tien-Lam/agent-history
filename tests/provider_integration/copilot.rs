use aghist::model::{ContentBlock, Provider, Role};
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::HistoryProvider;

use super::common;
use super::common::helpers::fixtures_dir;

#[test]
fn copilot_discover_sessions() {
    let provider = CopilotCliProvider::new(vec![fixtures_dir().join("copilot")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.id.0, "copilot-session-001");
    assert_eq!(s.provider, Provider::CopilotCli);
    assert_eq!(s.project_name.as_deref(), Some("myapp"));
    assert_eq!(s.message_count, 3);
}

#[test]
fn copilot_load_messages() {
    let provider = CopilotCliProvider::new(vec![fixtures_dir().join("copilot")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 3);

    assert_eq!(messages[0].role, Role::User);
    assert!(matches!(&messages[0].content[0], ContentBlock::Text(t) if t.contains("sort a list")));

    assert_eq!(messages[1].role, Role::Assistant);
    assert_eq!(messages[1].model.as_deref(), Some("gpt-4o"));
    let has_code = messages[1]
        .content
        .iter()
        .any(|c| matches!(c, ContentBlock::CodeBlock { .. }));
    assert!(has_code, "expected code block in assistant message");

    assert_eq!(messages[2].role, Role::Tool);
    assert!(
        matches!(&messages[2].content[0], ContentBlock::ToolUse(tc) if tc.name == "RunCommand")
    );
}

#[test]
fn copilot_tool_result_prefers_detailed_content() {
    let fixture = common::fixtures::copilot::CopilotFixtureBuilder::new()
        .add_session("copilot-tool-result")
        .raw_line(
            r#"{"id":"evt-result","type":"tool.execution_complete","timestamp":"2025-01-01T00:00:00Z","data":{"toolCallId":"call-1","success":true,"result":{"content":"short output","detailedContent":"long detailed output"}}}"#,
        )
        .done()
        .build();
    let provider = CopilotCliProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 1);
    assert!(
        matches!(&messages[0].content[0], ContentBlock::ToolResult(tr) if tr.tool_call_id == "call-1" && tr.output == "long detailed output")
    );
}
