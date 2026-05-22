use aghist::model::{ContentBlock, Provider, Role};
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::HistoryProvider;
use std::fs;

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

#[test]
fn copilot_tolerates_object_content_and_freeform_tool_result() {
    let fixture = common::fixtures::copilot::CopilotFixtureBuilder::new()
        .add_session("copilot-shape-drift")
        .raw_line(
            r#"{"id":"evt-object","type":"user.message","timestamp":"2025-01-01T00:00:00Z","content":{"text":"object content"},"model":{"id":"gpt-object"}}"#,
        )
        .raw_line(
            r#"{"id":"evt-result","type":"tool.execution_complete","timestamp":"2025-01-01T00:00:01Z","data":{"toolCallId":"call-1","success":true,"result":{"stdout":["line one","line two"]}}}"#,
        )
        .done()
        .build();
    let provider = CopilotCliProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].model.as_deref(), Some("gpt-object"));
    assert!(
        matches!(&messages[0].content[0], ContentBlock::Text(text) if text == "object content")
    );
    assert!(
        matches!(&messages[1].content[0], ContentBlock::ToolResult(tr) if tr.tool_call_id == "call-1" && tr.output.contains("line one"))
    );
}

#[test]
fn copilot_load_messages_with_stats_reports_parse_skips_and_empty_content() {
    let fixture = common::fixtures::copilot::CopilotFixtureBuilder::new()
        .add_session("copilot-stats")
        .raw_line("not json")
        .raw_line(r#"{"id":"evt-skip","type":"session.updated","timestamp":"2025-01-01T00:00:00Z"}"#)
        .raw_line(r#"{"id":"evt-empty","type":"user.message","timestamp":"2025-01-01T00:00:01Z","content":""}"#)
        .raw_line(r#"{"id":"evt-user","type":"user.message","timestamp":"2025-01-01T00:00:02Z","content":"kept"}"#)
        .done()
        .build();
    let provider = CopilotCliProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    let load = provider.load_messages_with_stats(&sessions[0]).unwrap();

    assert_eq!(load.messages.len(), 1);
    assert!(matches!(
        &load.messages[0].content[0],
        ContentBlock::Text(text) if text == "kept"
    ));
    assert_eq!(load.parse_stats.records_seen, 4);
    assert_eq!(load.parse_stats.parse_errors, 1);
    assert_eq!(load.parse_stats.skipped_records, 1);
    assert_eq!(load.parse_stats.empty_content, 1);
}

#[test]
fn copilot_tolerates_object_event_metadata_fields() {
    let fixture = common::fixtures::copilot::CopilotFixtureBuilder::new()
        .add_session("copilot-object-metadata")
        .raw_line(
            r#"{"id":{"id":"evt-user"},"type":{"type":"user.message"},"timestamp":{"value":1735689600000},"content":{"text":"object metadata content"}}"#,
        )
        .raw_line(
            r#"{"id":{"id":"evt-assistant"},"type":{"type":"assistant.message"},"timestamp":{"timestamp":"2025-01-01T00:00:01Z"},"content":"assistant reply","model":{"id":"gpt-object"},"usage":{"inputTokens":"4","outputTokens":{"tokens":5}}}"#,
        )
        .raw_line(
            r#"{"id":{"id":"evt-tool-start"},"type":{"type":"tool.execution_start"},"timestamp":{"timestamp":"2025-01-01T00:00:02Z"},"data":{"toolCallId":{"id":"call-object"},"toolName":{"name":"Run"},"arguments":{"cmd":"true"}}}"#,
        )
        .raw_line(
            r#"{"id":{"id":"evt-tool-result"},"type":{"type":"tool.execution_complete"},"timestamp":{"timestamp":"2025-01-01T00:00:03Z"},"data":{"toolCallId":{"id":"call-object"},"success":{"success":true},"result":{"content":"ok"}}}"#,
        )
        .done()
        .build();
    let provider = CopilotCliProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].message_count, 4);

    let messages = provider.load_messages(&sessions[0]).unwrap();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].id.0, "evt-user");
    assert_eq!(
        messages[0].timestamp.to_rfc3339(),
        "2025-01-01T00:00:00+00:00"
    );
    assert!(matches!(
        &messages[0].content[0],
        ContentBlock::Text(text) if text == "object metadata content"
    ));
    assert_eq!(messages[1].model.as_deref(), Some("gpt-object"));
    let usage = messages[1].token_usage.as_ref().unwrap();
    assert_eq!(usage.input_tokens, 4);
    assert_eq!(usage.output_tokens, 5);
    assert!(
        matches!(&messages[2].content[0], ContentBlock::ToolUse(tool) if tool.id == "call-object" && tool.name == "Run")
    );
    assert!(
        matches!(&messages[3].content[0], ContentBlock::ToolResult(result) if result.tool_call_id == "call-object" && result.success && result.output == "ok")
    );
}

#[test]
fn copilot_tolerates_object_workspace_metadata_fields() {
    let dir = tempfile::tempdir().unwrap();
    let session_dir = dir.path().join("copilot-yaml-object");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("workspace.yaml"),
        r#"
id:
  id: copilot-yaml-object
cwd:
  path: /tmp/objectapp
created_at:
  value: 1735689600000
updated_at:
  timestamp: "2025-01-01T00:00:01Z"
"#,
    )
    .unwrap();
    fs::write(
        session_dir.join("events.jsonl"),
        r#"{"id":"evt-user","type":"user.message","timestamp":"2025-01-01T00:00:00Z","content":"hello"}"#,
    )
    .unwrap();

    let provider = CopilotCliProvider::new(vec![dir.path().to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.id.0, "copilot-yaml-object");
    assert_eq!(session.project_name.as_deref(), Some("objectapp"));
    assert_eq!(session.started_at.to_rfc3339(), "2025-01-01T00:00:00+00:00");
    assert_eq!(
        session.ended_at.map(|ts| ts.to_rfc3339()).as_deref(),
        Some("2025-01-01T00:00:01+00:00")
    );
}
