use aghist::model::Role;
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;

use super::common::helpers::edge_cases_dir;

#[test]
fn claude_zero_message_session_skipped() {
    let provider = ClaudeCodeProvider::new(vec![edge_cases_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id.0, "session-corrupt");
}

#[test]
fn claude_corrupt_jsonl_lines_skipped() {
    let provider = ClaudeCodeProvider::new(vec![edge_cases_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, Role::User);
    assert_eq!(messages[1].role, Role::Assistant);
}

#[test]
fn copilot_empty_session_no_events() {
    let provider = CopilotCliProvider::new(vec![edge_cases_dir().join("copilot")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].message_count, 0);

    let messages = provider.load_messages(&sessions[0]).unwrap();
    assert!(messages.is_empty());
}

#[test]
fn codex_zero_message_session_skipped() {
    let provider = CodexCliProvider::new(vec![edge_cases_dir().join("codex")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id.0, "rollout-corrupt");
}

#[test]
fn codex_corrupt_jsonl_lines_skipped() {
    let provider = CodexCliProvider::new(vec![edge_cases_dir().join("codex")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, Role::User);
    assert_eq!(messages[1].role, Role::Assistant);
}

#[test]
fn gemini_zero_message_session_skipped() {
    let provider = GeminiCliProvider::new(vec![edge_cases_dir().join("gemini")]);
    let sessions = provider.discover_sessions().unwrap();

    assert!(sessions.is_empty());
}

#[test]
fn opencode_zero_message_session() {
    let provider = OpenCodeProvider::new(vec![edge_cases_dir().join("opencode")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id.0, "sess-empty");
    assert_eq!(sessions[0].message_count, 0);

    let messages = provider.load_messages(&sessions[0]).unwrap();
    assert!(messages.is_empty());
}
