use std::path::PathBuf;

use aghist::model::{ContentBlock, Provider, Role};
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn edge_cases_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edge_cases")
}

// ─── Claude Code ─────────────────────────────────────────────────────────────

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

    // First message: user text
    assert_eq!(messages[0].role, Role::User);
    assert_eq!(messages[0].id.0, "msg-001");
    assert!(matches!(&messages[0].content[0], ContentBlock::Text(t) if t.contains("Fix the build")));

    // Second message: assistant with text + tool_use
    assert_eq!(messages[1].role, Role::Assistant);
    assert!(matches!(&messages[1].content[0], ContentBlock::Text(t) if t.contains("fix the build")));
    assert!(matches!(&messages[1].content[1], ContentBlock::ToolUse(tc) if tc.name == "Read"));

    // Third message: user with tool_result
    assert_eq!(messages[2].role, Role::User);
    assert!(matches!(&messages[2].content[0], ContentBlock::ToolResult(tr) if tr.success && tr.output.contains("fn main")));

    // Fourth message: assistant with thinking + text + code block
    assert_eq!(messages[3].role, Role::Assistant);
    let has_thinking = messages[3].content.iter().any(|c| matches!(c, ContentBlock::Thinking(_)));
    let has_code = messages[3].content.iter().any(|c| matches!(c, ContentBlock::CodeBlock { .. }));
    assert!(has_thinking, "expected thinking block");
    assert!(has_code, "expected code block from markdown");
}

// ─── Copilot CLI ─────────────────────────────────────────────────────────────

#[test]
fn copilot_discover_sessions() {
    let provider = CopilotCliProvider::new(vec![fixtures_dir().join("copilot")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.id.0, "copilot-session-001");
    assert_eq!(s.provider, Provider::CopilotCli);
    assert_eq!(s.project_name.as_deref(), Some("myapp"));
    assert_eq!(s.message_count, 2); // user.message + assistant.message
}

#[test]
fn copilot_load_messages() {
    let provider = CopilotCliProvider::new(vec![fixtures_dir().join("copilot")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert_eq!(messages.len(), 3); // user + assistant + tool

    assert_eq!(messages[0].role, Role::User);
    assert!(matches!(&messages[0].content[0], ContentBlock::Text(t) if t.contains("sort a list")));

    assert_eq!(messages[1].role, Role::Assistant);
    assert_eq!(messages[1].model.as_deref(), Some("gpt-4o"));
    let has_code = messages[1].content.iter().any(|c| matches!(c, ContentBlock::CodeBlock { .. }));
    assert!(has_code, "expected code block in assistant message");

    assert_eq!(messages[2].role, Role::Tool);
    assert!(matches!(&messages[2].content[0], ContentBlock::ToolUse(tc) if tc.name == "RunCommand"));
}

// ─── Gemini CLI ──────────────────────────────────────────────────────────────

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
    let has_code = messages[1].content.iter().any(|c| matches!(c, ContentBlock::CodeBlock { .. }));
    let has_thinking = messages[1].content.iter().any(|c| matches!(c, ContentBlock::Thinking(_)));
    let has_tool = messages[1].content.iter().any(|c| matches!(c, ContentBlock::ToolUse(_)));
    assert!(has_code, "expected code block");
    assert!(has_thinking, "expected thinking block");
    assert!(has_tool, "expected tool call");
}

// ─── Codex CLI ───────────────────────────────────────────────────────────────

#[test]
fn codex_discover_sessions() {
    let provider = CodexCliProvider::new(vec![fixtures_dir().join("codex")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.id.0, "rollout-test123");
    assert_eq!(s.provider, Provider::CodexCli);
    assert_eq!(s.message_count, 3); // 1 user + 2 assistant
    assert!(s.summary.as_ref().unwrap().contains("error handling"));
}

#[test]
fn codex_load_messages() {
    let provider = CodexCliProvider::new(vec![fixtures_dir().join("codex")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    // user + assistant + tool_use + error + assistant = 5
    assert_eq!(messages.len(), 5);

    assert_eq!(messages[0].role, Role::User);
    assert_eq!(messages[1].role, Role::Assistant);
    assert_eq!(messages[2].role, Role::Tool);
    assert!(matches!(&messages[2].content[0], ContentBlock::ToolUse(tc) if tc.name == "EditFile"));

    // Error entry becomes System message
    assert_eq!(messages[3].role, Role::System);
    assert!(matches!(&messages[3].content[0], ContentBlock::Error(e) if e.contains("File not found")));

    assert_eq!(messages[4].role, Role::Assistant);
}

// ─── OpenCode ────────────────────────────────────────────────────────────────

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
    assert!(matches!(&messages[0].content[0], ContentBlock::Text(t) if t.contains("connection pool")));

    assert_eq!(messages[1].role, Role::Assistant);
    let has_text = messages[1].content.iter().any(|c| matches!(c, ContentBlock::Text(_)));
    let has_diff = messages[1].content.iter().any(|c| {
        matches!(c, ContentBlock::CodeBlock { language, .. } if language.as_deref().unwrap_or("").contains("diff"))
    });
    assert!(has_text, "expected text block");
    assert!(has_diff, "expected diff code block");
}

// ─── Edge Cases ──────────────────────────────────────────────────────────────

#[test]
fn claude_zero_message_session_skipped() {
    let provider = ClaudeCodeProvider::new(vec![edge_cases_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();

    // session-empty has only a system message (no user/assistant), should be skipped
    // session-corrupt has corrupt lines but 2 valid user+assistant messages
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id.0, "session-corrupt");
}

#[test]
fn claude_corrupt_jsonl_lines_skipped() {
    let provider = ClaudeCodeProvider::new(vec![edge_cases_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();
    let messages = provider.load_messages(&sessions[0]).unwrap();

    // 2 valid messages survive from the corrupt file (msg-c01 user, msg-c02 assistant)
    // msg-c03 has empty content so gets skipped
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

    // rollout-empty has only a system entry, should be skipped
    // rollout-corrupt has corrupt lines but 2 valid entries
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

    // session-bad has only a system message, should be skipped
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

#[test]
fn nonexistent_base_dir_returns_empty() {
    let fake_dir = PathBuf::from("/nonexistent/path/that/does/not/exist");

    let claude = ClaudeCodeProvider::new(vec![fake_dir.clone()]);
    assert!(claude.discover_sessions().unwrap().is_empty());

    let copilot = CopilotCliProvider::new(vec![fake_dir.clone()]);
    assert!(copilot.discover_sessions().unwrap().is_empty());

    let gemini = GeminiCliProvider::new(vec![fake_dir.clone()]);
    assert!(gemini.discover_sessions().unwrap().is_empty());

    let codex = CodexCliProvider::new(vec![fake_dir.clone()]);
    assert!(codex.discover_sessions().unwrap().is_empty());

    let opencode = OpenCodeProvider::new(vec![fake_dir]);
    assert!(opencode.discover_sessions().unwrap().is_empty());
}
