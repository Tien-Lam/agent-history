use std::path::Path;

use aghist::model::Provider;
use aghist::provider::registry::provider_from_dirs;
use aghist::provider::HistoryProvider;
use tempfile::TempDir;

use super::super::fixtures;
use super::super::helpers::fixtures_dir;

pub struct ProviderCase {
    pub label: &'static str,
    pub provider: Box<dyn HistoryProvider>,
    pub expected_provider: Provider,
    pub expected_sessions: Option<usize>,
    pub expected_messages_per_session: Option<usize>,
}

pub struct GeneratedProviderCases {
    _dirs: Vec<TempDir>,
    cases: Vec<ProviderCase>,
}

impl GeneratedProviderCases {
    pub fn cases(&self) -> &[ProviderCase] {
        &self.cases
    }

    pub fn providers(&self) -> Vec<Provider> {
        self.cases
            .iter()
            .map(|case| case.expected_provider)
            .collect()
    }
}

pub fn missing_dir_provider_cases(base: &Path) -> Vec<ProviderCase> {
    Provider::all()
        .iter()
        .map(|provider| ProviderCase {
            label: provider.slug(),
            provider: provider_from_dirs(*provider, vec![base.join(format!("missing-{provider}"))]),
            expected_provider: *provider,
            expected_sessions: Some(0),
            expected_messages_per_session: None,
        })
        .collect()
}

pub fn generated_provider_cases(
    n_sessions: usize,
    messages_per_session: usize,
) -> GeneratedProviderCases {
    let (dirs, providers) =
        fixtures::generated::all_generated_providers(n_sessions, messages_per_session);
    let cases = providers
        .into_iter()
        .map(|provider| {
            let expected_provider = provider.provider();
            ProviderCase {
                label: expected_provider.slug(),
                provider,
                expected_provider,
                expected_sessions: Some(n_sessions),
                expected_messages_per_session: Some(messages_per_session),
            }
        })
        .collect();
    GeneratedProviderCases { _dirs: dirs, cases }
}

pub fn static_fixture_provider_cases() -> Vec<ProviderCase> {
    [
        ("claude", Provider::ClaudeCode, "claude"),
        ("copilot", Provider::CopilotCli, "copilot"),
        ("copilot_v2", Provider::CopilotCli, "copilot_v2"),
        ("codex", Provider::CodexCli, "codex"),
        ("codex_v2", Provider::CodexCli, "codex_v2"),
        ("opencode", Provider::OpenCode, "opencode"),
        ("opencode_v2", Provider::OpenCode, "opencode_v2"),
        ("gemini", Provider::GeminiCli, "gemini"),
    ]
    .into_iter()
    .map(|(label, expected_provider, fixture_dir)| ProviderCase {
        label,
        provider: provider_from_dirs(expected_provider, vec![fixtures_dir().join(fixture_dir)]),
        expected_provider,
        expected_sessions: None,
        expected_messages_per_session: None,
    })
    .collect()
}
