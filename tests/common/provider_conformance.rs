use std::path::Path;

use aghist::model::Provider;
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::cursor::CursorProvider;
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;
use tempfile::TempDir;

use super::fixtures;

pub struct ProviderCase {
    pub label: &'static str,
    pub provider: Box<dyn HistoryProvider>,
    pub expected_provider: Provider,
    pub expected_sessions: Option<usize>,
    pub expected_messages_per_session: Option<usize>,
}

pub fn missing_dir_provider_cases(base: &Path) -> Vec<ProviderCase> {
    vec![
        ProviderCase {
            label: "claude-code",
            provider: Box::new(ClaudeCodeProvider::new(vec![base.join("missing-claude")])),
            expected_provider: Provider::ClaudeCode,
            expected_sessions: Some(0),
            expected_messages_per_session: None,
        },
        ProviderCase {
            label: "copilot-cli",
            provider: Box::new(CopilotCliProvider::new(vec![base.join("missing-copilot")])),
            expected_provider: Provider::CopilotCli,
            expected_sessions: Some(0),
            expected_messages_per_session: None,
        },
        ProviderCase {
            label: "gemini-cli",
            provider: Box::new(GeminiCliProvider::new(vec![base.join("missing-gemini")])),
            expected_provider: Provider::GeminiCli,
            expected_sessions: Some(0),
            expected_messages_per_session: None,
        },
        ProviderCase {
            label: "codex-cli",
            provider: Box::new(CodexCliProvider::new(vec![base.join("missing-codex")])),
            expected_provider: Provider::CodexCli,
            expected_sessions: Some(0),
            expected_messages_per_session: None,
        },
        ProviderCase {
            label: "opencode",
            provider: Box::new(OpenCodeProvider::new(vec![base.join("missing-opencode")])),
            expected_provider: Provider::OpenCode,
            expected_sessions: Some(0),
            expected_messages_per_session: None,
        },
        ProviderCase {
            label: "cursor",
            provider: Box::new(CursorProvider::new(vec![base.join("missing-cursor")])),
            expected_provider: Provider::Cursor,
            expected_sessions: Some(0),
            expected_messages_per_session: None,
        },
    ]
}

pub fn generated_provider_cases(
    n_sessions: usize,
    messages_per_session: usize,
) -> (Vec<TempDir>, Vec<ProviderCase>) {
    let (dirs, providers) = fixtures::all_generated_providers(n_sessions, messages_per_session);
    let cases = providers
        .into_iter()
        .map(|provider| {
            let expected_provider = provider.provider();
            ProviderCase {
                label: provider_label(expected_provider),
                provider,
                expected_provider,
                expected_sessions: Some(n_sessions),
                expected_messages_per_session: Some(messages_per_session),
            }
        })
        .collect();
    (dirs, cases)
}

pub fn assert_missing_dir_discovers_empty(case: &ProviderCase) {
    assert_eq!(
        case.provider.provider(),
        case.expected_provider,
        "{} provider identity drifted",
        case.label
    );
    let sessions = case
        .provider
        .discover_sessions()
        .unwrap_or_else(|e| panic!("{} discovery failed: {e}", case.label));
    assert_eq!(
        sessions.len(),
        case.expected_sessions.unwrap_or(0),
        "{} should discover no sessions for a missing root",
        case.label
    );
}

pub fn assert_discover_load_roundtrip(case: &ProviderCase) {
    assert_eq!(
        case.provider.provider(),
        case.expected_provider,
        "{} provider identity drifted",
        case.label
    );
    let sessions = case
        .provider
        .discover_sessions()
        .unwrap_or_else(|e| panic!("{} discovery failed: {e}", case.label));
    if let Some(expected) = case.expected_sessions {
        assert_eq!(
            sessions.len(),
            expected,
            "{} discovered an unexpected session count",
            case.label
        );
    }

    for session in &sessions {
        assert_eq!(
            session.provider, case.expected_provider,
            "{} discovered a session tagged with the wrong provider",
            case.label
        );
        let messages = case
            .provider
            .load_messages(session)
            .unwrap_or_else(|e| panic!("{} failed to load {}: {e}", case.label, session.id.0));
        if let Some(expected) = case.expected_messages_per_session {
            assert_eq!(
                messages.len(),
                expected,
                "{} loaded an unexpected message count for {}",
                case.label,
                session.id.0
            );
        } else {
            assert!(
                !messages.is_empty(),
                "{} should load messages for {}",
                case.label,
                session.id.0
            );
        }
    }
}

fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::ClaudeCode => "claude-code",
        Provider::CopilotCli => "copilot-cli",
        Provider::GeminiCli => "gemini-cli",
        Provider::CodexCli => "codex-cli",
        Provider::OpenCode => "opencode",
        Provider::Cursor => "cursor",
        Provider::Aider => "aider",
        Provider::Cline => "cline",
        Provider::ContinueDev => "continue-dev",
        Provider::ZedAi => "zed-ai",
    }
}
