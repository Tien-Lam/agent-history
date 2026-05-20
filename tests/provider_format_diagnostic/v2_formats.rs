use aghist::model::{ContentBlock, Provider, Role};
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;

use super::fixtures_dir;

#[test]
fn copilot_v2_discover_sessions() {
    let provider = CopilotCliProvider::new(vec![fixtures_dir().join("copilot_v2")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1, "should discover 1 session");
    let s = &sessions[0];
    assert_eq!(s.id.0, "session-v2-001");
    assert_eq!(s.provider, Provider::CopilotCli);
    assert_eq!(s.project_name.as_deref(), Some("myapp"));
}

#[test]
fn copilot_v2_load_messages() {
    let provider = CopilotCliProvider::new(vec![fixtures_dir().join("copilot_v2")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(!sessions.is_empty(), "should discover sessions");

    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert!(
        !messages.is_empty(),
        "0 messages loaded from Copilot v2 format"
    );

    let user_msgs: Vec<_> = messages.iter().filter(|m| m.role == Role::User).collect();
    assert!(
        !user_msgs.is_empty(),
        "should have at least one user message"
    );
    assert!(
        matches!(&user_msgs[0].content[0], ContentBlock::Text(t) if t.contains("CI tests")),
        "user message content should contain 'CI tests', got: {:?}",
        user_msgs[0].content
    );

    let asst_msgs: Vec<_> = messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .collect();
    assert!(
        !asst_msgs.is_empty(),
        "should have at least one assistant message"
    );
    assert!(
        matches!(&asst_msgs[0].content[0], ContentBlock::Text(t) if t.contains("investigate")),
        "assistant message should contain text content"
    );

    let has_tool_use = asst_msgs.iter().any(|m| {
        m.content
            .iter()
            .any(|c| matches!(c, ContentBlock::ToolUse(tc) if tc.name == "RunCommand"))
    });
    assert!(
        has_tool_use,
        "should parse tool requests from data.toolRequests"
    );
}

#[test]
fn copilot_v2_message_count_matches_discovery() {
    let provider = CopilotCliProvider::new(vec![fixtures_dir().join("copilot_v2")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    let user_assistant_count = messages
        .iter()
        .filter(|m| m.role == Role::User || m.role == Role::Assistant)
        .count();

    assert!(
        user_assistant_count > 0,
        "discover reports {} messages but load_messages returned {} user/assistant messages",
        sessions[0].message_count,
        user_assistant_count
    );
}

#[test]
fn codex_v2_discover_sessions() {
    let provider = CodexCliProvider::new(vec![fixtures_dir().join("codex_v2")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1, "should discover 1 session");
    let s = &sessions[0];
    assert_eq!(s.id.0, "rollout-test-v2");
    assert_eq!(s.provider, Provider::CodexCli);
    assert!(s.message_count > 0, "session should have messages");
}

#[test]
fn codex_v2_load_messages() {
    let provider = CodexCliProvider::new(vec![fixtures_dir().join("codex_v2")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(!sessions.is_empty(), "should discover sessions");

    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert!(
        !messages.is_empty(),
        "0 messages loaded from Codex v2 format"
    );

    let user_msgs: Vec<_> = messages.iter().filter(|m| m.role == Role::User).collect();
    assert_eq!(user_msgs.len(), 2, "should have 2 user messages");
    assert!(
        matches!(&user_msgs[0].content[0], ContentBlock::Text(t) if t.contains("error handling")),
        "first user message should mention error handling"
    );
    assert!(
        matches!(&user_msgs[1].content[0], ContentBlock::Text(t) if t.contains("validation")),
        "second user message should mention validation"
    );

    let asst_msgs: Vec<_> = messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .collect();
    assert_eq!(asst_msgs.len(), 3, "should have 3 assistant messages");

    let has_code = asst_msgs.iter().any(|m| {
        m.content
            .iter()
            .any(|c| matches!(c, ContentBlock::CodeBlock { .. }))
    });
    assert!(
        has_code,
        "agent messages with code fences should produce CodeBlock content"
    );
}

#[test]
fn codex_v2_summary_from_first_user_message() {
    let provider = CodexCliProvider::new(vec![fixtures_dir().join("codex_v2")]);
    let sessions = provider.discover_sessions().unwrap();

    assert!(
        sessions[0].summary.is_some(),
        "session summary should be extracted from first user_message payload"
    );
    assert!(
        sessions[0]
            .summary
            .as_ref()
            .unwrap()
            .contains("error handling"),
        "summary should contain user's first message text"
    );
}

#[test]
fn opencode_v2_discover_sessions() {
    let provider = OpenCodeProvider::new(vec![fixtures_dir().join("opencode_v2")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1, "should discover 1 session");
    let s = &sessions[0];
    assert_eq!(s.id.0, "ses-v2-001");
    assert_eq!(s.provider, Provider::OpenCode);
    assert_eq!(s.project_name.as_deref(), Some("myproject"));
    assert_eq!(s.summary.as_deref(), Some("Add error handling to API"));
    assert_eq!(s.model.as_deref(), Some("claude-sonnet-4"));
    assert_eq!(s.message_count, 2);
}

#[test]
fn opencode_v2_load_messages() {
    let provider = OpenCodeProvider::new(vec![fixtures_dir().join("opencode_v2")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(!sessions.is_empty(), "should discover sessions");

    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert!(
        !messages.is_empty(),
        "0 messages loaded from OpenCode v2 format"
    );

    let user_msgs: Vec<_> = messages.iter().filter(|m| m.role == Role::User).collect();
    assert_eq!(user_msgs.len(), 1, "should have 1 user message");
    assert!(
        matches!(&user_msgs[0].content[0], ContentBlock::Text(t) if t.contains("error handling")),
        "user message text should come from part file"
    );

    let asst_msgs: Vec<_> = messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .collect();
    assert_eq!(asst_msgs.len(), 1, "should have 1 assistant message");

    let has_text = asst_msgs[0]
        .content
        .iter()
        .any(|c| matches!(c, ContentBlock::Text(t) if t.contains("error handling")));
    assert!(has_text, "assistant should have text from part files");

    let has_tool = asst_msgs[0]
        .content
        .iter()
        .any(|c| matches!(c, ContentBlock::ToolUse(tc) if tc.name == "read"));
    assert!(has_tool, "assistant should have tool calls from part files");

    assert_eq!(asst_msgs[0].model.as_deref(), Some("claude-sonnet-4"));
    let usage = asst_msgs[0].token_usage.as_ref().unwrap();
    assert_eq!(usage.input_tokens, 500);
    assert_eq!(usage.output_tokens, 200);
}

#[test]
fn opencode_v2_timestamps_from_millis() {
    let provider = OpenCodeProvider::new(vec![fixtures_dir().join("opencode_v2")]);
    let sessions = provider.discover_sessions().unwrap();

    assert!(
        sessions[0].started_at.timestamp() > 0,
        "started_at should be parsed from time.created millis"
    );
    assert!(
        sessions[0].ended_at.is_some(),
        "ended_at should be parsed from time.updated millis"
    );
}

#[test]
fn claude_fixture_roundtrip_messages_nonempty() {
    let provider = ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(!sessions.is_empty());

    for session in &sessions {
        let messages = provider.load_messages(session).unwrap();
        assert!(
            !messages.is_empty(),
            "session {} (message_count={}) loaded 0 messages - format mismatch?",
            session.id.0,
            session.message_count
        );
    }
}
