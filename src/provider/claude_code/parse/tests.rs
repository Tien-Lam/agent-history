use super::*;
use tempfile::TempDir;

#[test]
fn decode_project_name_basic() {
    assert_eq!(
        decode_project_name("V--Projects-agent-history"),
        "V:/Projects-agent-history"
    );
}

#[test]
fn parse_session_messages_tolerates_object_shaped_fields() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("session.jsonl");
    std::fs::write(
        &path,
        [
            r#"{"type":{"type":"user"},"uuid":{"id":"u1"},"timestamp":{"timestamp":"2026-01-01T00:00:00Z"},"message":{"content":{"text":"object message"}}}"#,
            r#"{"type":"assistant","uuid":{"id":"a1"},"timestamp":"2026-01-01T00:00:01Z","message":{"model":{"id":"claude-object"},"usage":{"input_tokens":"7","output_tokens":{"value":3}},"content":[{"type":{"type":"text"},"text":{"content":"assistant text"}},{"type":"thinking","thinking":{"text":"thought"}},{"type":"tool_use","id":{"id":"call1"},"name":{"name":"Read"},"input":{"file":"a"}}]}}"#,
            r#"{"type":"user","uuid":"u2","timestamp":"2026-01-01T00:00:02Z","message":{"content":[{"type":"tool_result","tool_use_id":{"id":"call1"},"is_error":"false","content":{"text":"tool ok"}}]}}"#,
        ]
        .join("\n"),
    )
    .unwrap();

    let messages = parse_session_messages(&path).unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].id.0, "u1");
    assert_eq!(messages[0].role, Role::User);
    assert!(matches!(
        &messages[0].content[0],
        ContentBlock::Text(text) if text == "object message"
    ));

    let assistant = &messages[1];
    assert_eq!(assistant.id.0, "a1");
    assert_eq!(assistant.role, Role::Assistant);
    assert_eq!(assistant.model.as_deref(), Some("claude-object"));
    assert_eq!(assistant.token_usage.as_ref().unwrap().input_tokens, 7);
    assert_eq!(assistant.token_usage.as_ref().unwrap().output_tokens, 3);
    assert!(assistant
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::Thinking(text) if text == "thought")));
    assert!(assistant
        .content
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolUse(tool) if tool.id == "call1" && tool.name == "Read")));

    assert!(matches!(
        &messages[2].content[0],
        ContentBlock::ToolResult(result)
            if result.tool_call_id == "call1" && result.output == "tool ok"
    ));
}

#[test]
fn build_session_metadata_tolerates_history_shape_drift() {
    let tmp = TempDir::new().unwrap();
    let history_path = tmp.path().join("history.jsonl");
    std::fs::write(
        &history_path,
        r#"{"sessionId":{"id":"session-1"},"display":{"text":"History summary"},"timestamp":{"value":1767225600000}}"#,
    )
    .unwrap();
    let history_entries = parse_history_index(&history_path).unwrap();

    let session_path = tmp.path().join("session-1.jsonl");
    std::fs::write(
        &session_path,
        r#"{"type":{"type":"assistant"},"uuid":{"id":"a1"},"timestamp":{"timestamp":"2026-01-01T00:00:01Z"},"gitBranch":{"branch":"main"},"cwd":{"path":"/home/me/project"},"message":{"model":{"id":"claude-model"},"usage":{"input_tokens":{"tokens":5},"output_tokens":"6"},"content":"hello"}}"#,
    )
    .unwrap();

    let session =
        build_session_metadata(&session_path, "session-1", "project", &history_entries).unwrap();
    assert_eq!(session.summary.as_deref(), Some("History summary"));
    assert_eq!(session.git_branch.as_deref(), Some("main"));
    assert_eq!(
        session.project_path.as_ref().unwrap(),
        &PathBuf::from("/home/me/project")
    );
    assert_eq!(session.model.as_deref(), Some("claude-model"));
    assert_eq!(session.token_usage.as_ref().unwrap().input_tokens, 5);
    assert_eq!(session.token_usage.as_ref().unwrap().output_tokens, 6);
}
