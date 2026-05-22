use std::fs;

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

#[test]
fn gemini_tolerates_shape_drift_without_dropping_session() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    fs::write(
        base.join("projects.json"),
        r#"{"projects":{"/tmp/drift":"test-project"}}"#,
    )
    .unwrap();
    let chats = base.join("tmp").join("test-project").join("chats");
    fs::create_dir_all(&chats).unwrap();
    fs::write(
        chats.join("session-drift.json"),
        r#"{
          "sessionId":"gemini-drift",
          "messages":[
            {"id":"missing-type","timestamp":"2025-01-01T00:00:00Z","content":"ignored"},
            {"id":"gm-user","timestamp":"2025-01-01T00:00:01Z","type":"user","content":{"text":"object user text"},"displayContent":{"unexpected":"object"}},
            {"id":"gm-assistant","timestamp":"2025-01-01T00:00:02Z","type":"gemini","content":[{"text":"assistant text"}],"thoughts":{"unexpected":"object"},"toolCalls":{"unexpected":"object"}}
          ]
        }"#,
    )
    .unwrap();

    let provider = GeminiCliProvider::new(vec![base.to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id.0, "gemini-drift");
    assert_eq!(sessions[0].message_count, 2);
    assert_eq!(sessions[0].summary.as_deref(), Some("object user text"));
    assert_eq!(
        sessions[0].started_at.to_rfc3339(),
        "2025-01-01T00:00:01+00:00"
    );
    assert_eq!(
        sessions[0].ended_at.unwrap().to_rfc3339(),
        "2025-01-01T00:00:02+00:00"
    );

    let messages = provider.load_messages(&sessions[0]).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, Role::User);
    assert!(matches!(&messages[0].content[0], ContentBlock::Text(t) if t == "object user text"));
    assert_eq!(messages[1].role, Role::Assistant);
}

#[test]
fn gemini_tolerates_object_message_metadata_fields() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    fs::write(
        base.join("projects.json"),
        r#"{"projects":{"/tmp/object":"object-project"}}"#,
    )
    .unwrap();
    let chats = base.join("tmp").join("object-project").join("chats");
    fs::create_dir_all(&chats).unwrap();
    fs::write(
        chats.join("session-object.json"),
        r#"{
          "sessionId":{"id":"gemini-object"},
          "startTime":{"timestamp":"2025-01-01T00:00:00Z"},
          "lastUpdated":{"timestamp":"2025-01-01T00:00:02Z"},
          "messages":[
            "skip malformed message",
            {
              "id":{"id":"gm-user"},
              "timestamp":{"timestamp":"2025-01-01T00:00:01Z"},
              "type":{"type":"user"},
              "content":[{"text":{"content":"display object text"}}],
              "displayContent":[{"text":{"text":"display object text"}}],
              "tokens":{"input":"4","output":{"value":5}}
            },
            {
              "id":{"id":"gm-assistant"},
              "timestamp":{"timestamp":"2025-01-01T00:00:02Z"},
              "type":{"type":"gemini"},
              "content":[{"text":{"content":"assistant object text"}}],
              "thoughts":[{"description":{"text":"thinking object"}}],
              "toolCalls":[{"id":{"id":"tool-1"},"name":{"name":"inspect"},"args":{"path":"src/main.rs"},"response":{"output":{"text":"tool ok"}}}],
              "model":{"id":"gemini-object-model"},
              "tokens":{"input":{"tokens":6},"output":"7","cached":{"count":2}}
            }
          ]
        }"#,
    )
    .unwrap();

    let provider = GeminiCliProvider::new(vec![base.to_path_buf()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.id.0, "gemini-object");
    assert_eq!(session.project_name.as_deref(), Some("object-project"));
    assert_eq!(session.model.as_deref(), Some("gemini-object-model"));
    assert_eq!(session.summary.as_deref(), Some("display object text"));
    assert_eq!(session.message_count, 2);
    let usage = session.token_usage.as_ref().unwrap();
    assert_eq!(usage.input_tokens, 10);
    assert_eq!(usage.output_tokens, 12);

    let messages = provider.load_messages(session).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id.0, "gm-user");
    assert_eq!(messages[0].role, Role::User);
    assert!(matches!(
        &messages[0].content[0],
        ContentBlock::Text(text) if text == "display object text"
    ));

    let assistant = &messages[1];
    assert_eq!(assistant.id.0, "gm-assistant");
    assert_eq!(assistant.model.as_deref(), Some("gemini-object-model"));
    assert!(assistant
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Thinking(text) if text == "thinking object")));
    assert!(
        assistant
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolUse(tool) if tool.id == "tool-1" && tool.name == "inspect"))
    );
    assert!(
        assistant
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::ToolResult(result) if result.tool_call_id == "tool-1" && result.output.contains("tool ok")))
    );
}
