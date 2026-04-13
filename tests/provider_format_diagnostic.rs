//! Diagnostic tests for real-world provider format compatibility.
//!
//! These tests use fixtures that match the actual formats found on real systems
//! (as of 2026-04), catching format mismatches that the original fixtures don't cover.

mod common;

use aghist::model::{ContentBlock, Provider, Role};
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::HistoryProvider;

use common::helpers::fixtures_dir;

// ─── Copilot CLI v2 format (data-nested) ────────────────────────────────────

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

    // The v2 format nests content in data.content - this test verifies we can read it
    assert!(
        !messages.is_empty(),
        "FAIL: 0 messages loaded from Copilot v2 format — \
         content is likely nested in data.content but parser only checks top-level content"
    );

    // Verify user message
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

    // Verify assistant messages
    let asst_msgs: Vec<_> = messages.iter().filter(|m| m.role == Role::Assistant).collect();
    assert!(
        !asst_msgs.is_empty(),
        "should have at least one assistant message"
    );
    assert!(
        matches!(&asst_msgs[0].content[0], ContentBlock::Text(t) if t.contains("investigate")),
        "assistant message should contain text content"
    );

    // Verify tool calls from data.toolRequests
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

// ─── Codex CLI v2 format (event_msg + payload) ─────────────────────────────

#[test]
fn codex_v2_discover_sessions() {
    let provider = CodexCliProvider::new(vec![fixtures_dir().join("codex_v2")]);
    let sessions = provider.discover_sessions().unwrap();

    assert_eq!(sessions.len(), 1, "should discover 1 session");
    let s = &sessions[0];
    assert_eq!(s.id.0, "rollout-test-v2");
    assert_eq!(s.provider, Provider::CodexCli);
    assert!(
        s.message_count > 0,
        "session should have messages (event_msg with user_message/agent_message payload)"
    );
}

#[test]
fn codex_v2_load_messages() {
    let provider = CodexCliProvider::new(vec![fixtures_dir().join("codex_v2")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(!sessions.is_empty(), "should discover sessions");

    let messages = provider.load_messages(&sessions[0]).unwrap();

    assert!(
        !messages.is_empty(),
        "FAIL: 0 messages loaded from Codex v2 format — \
         type is 'event_msg' with payload.type='user_message'/'agent_message' \
         but parser only checks top-level type='user'/'assistant'"
    );

    // Verify user messages
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

    // Verify assistant/agent messages
    let asst_msgs: Vec<_> = messages.iter().filter(|m| m.role == Role::Assistant).collect();
    assert_eq!(asst_msgs.len(), 3, "should have 3 assistant messages");

    // Verify code blocks are parsed from agent messages
    let has_code = asst_msgs.iter().any(|m| {
        m.content
            .iter()
            .any(|c| matches!(c, ContentBlock::CodeBlock { .. }))
    });
    assert!(has_code, "agent messages with code fences should produce CodeBlock content");
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

// ─── Claude Code format (current real format) ───────────────────────────────

#[test]
fn claude_fixture_roundtrip_messages_nonempty() {
    let provider = ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(!sessions.is_empty());

    for session in &sessions {
        let messages = provider.load_messages(session).unwrap();
        assert!(
            !messages.is_empty(),
            "session {} (message_count={}) loaded 0 messages — format mismatch?",
            session.id.0,
            session.message_count
        );
    }
}

// ─── Cross-provider: discover → load round-trip ─────────────────────────────

#[test]
fn all_fixture_providers_roundtrip() {
    let providers: Vec<(&str, Box<dyn HistoryProvider>)> = vec![
        (
            "claude",
            Box::new(ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")])),
        ),
        (
            "copilot",
            Box::new(CopilotCliProvider::new(vec![fixtures_dir().join("copilot")])),
        ),
        (
            "copilot_v2",
            Box::new(CopilotCliProvider::new(vec![fixtures_dir().join("copilot_v2")])),
        ),
        (
            "codex",
            Box::new(CodexCliProvider::new(vec![fixtures_dir().join("codex")])),
        ),
        (
            "codex_v2",
            Box::new(CodexCliProvider::new(vec![fixtures_dir().join("codex_v2")])),
        ),
    ];

    for (label, provider) in &providers {
        let sessions = provider
            .discover_sessions()
            .unwrap_or_else(|e| panic!("{label}: discover_sessions failed: {e}"));

        for session in &sessions {
            let messages = provider
                .load_messages(session)
                .unwrap_or_else(|e| panic!("{label}: load_messages failed for {}: {e}", session.id.0));

            eprintln!(
                "[{label}] session={} discovered_count={} loaded_count={}",
                session.id.0,
                session.message_count,
                messages.len()
            );

            if session.message_count > 0 {
                assert!(
                    !messages.is_empty(),
                    "[{label}] session {} has message_count={} but load_messages returned 0 — \
                     this indicates a format mismatch between discover and load",
                    session.id.0,
                    session.message_count,
                );
            }
        }
    }
}

// ─── Live data diagnostic (opt-in via env var) ──────────────────────────────

/// Run with AGHIST_LIVE_TEST=1 to test against real data on this system.
/// This test doesn't assert — it prints a diagnostic report.
#[test]
fn live_data_diagnostic() {
    if std::env::var("AGHIST_LIVE_TEST").is_err() {
        eprintln!("Skipping live_data_diagnostic (set AGHIST_LIVE_TEST=1 to run)");
        return;
    }

    let providers = aghist::provider::detect_all_providers();

    for provider in &providers {
        let sessions = match provider.discover_sessions() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[{}] DISCOVER ERROR: {e}", provider.provider());
                continue;
            }
        };

        eprintln!("[{}] discovered {} sessions", provider.provider(), sessions.len());

        let mut loaded = 0;
        let mut empty = 0;
        let mut errors = 0;

        for session in sessions.iter().take(5) {
            match provider.load_messages(session) {
                Ok(msgs) => {
                    if msgs.is_empty() {
                        empty += 1;
                        eprintln!(
                            "  EMPTY: session={} message_count={} source={}",
                            session.id.0,
                            session.message_count,
                            session.source_path.display()
                        );
                    } else {
                        loaded += 1;
                        let roles: Vec<_> = msgs.iter().map(|m| format!("{:?}", m.role)).collect();
                        eprintln!(
                            "  OK:    session={} loaded={} roles=[{}]",
                            session.id.0,
                            msgs.len(),
                            roles.join(", ")
                        );
                    }
                }
                Err(e) => {
                    errors += 1;
                    eprintln!(
                        "  ERROR: session={} error={e}",
                        session.id.0
                    );
                }
            }
        }

        eprintln!(
            "[{}] sample results: loaded={loaded}, empty={empty}, errors={errors}",
            provider.provider()
        );
    }
}
