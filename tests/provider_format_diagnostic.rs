//! Diagnostic tests for real-world provider format compatibility.
//!
//! These tests use fixtures that match actual formats found on real systems.

mod common;

use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;
use aghist::provider_diagnostic::{analyze_provider, ProviderDiagnostic};

use common::helpers::fixtures_dir;

#[path = "provider_format_diagnostic/cross_provider.rs"]
mod cross_provider;
#[path = "provider_format_diagnostic/diagnostics.rs"]
mod diagnostics;
#[path = "provider_format_diagnostic/live.rs"]
mod live;
#[path = "provider_format_diagnostic/v2_formats.rs"]
mod v2_formats;

fn fixture_provider_set() -> Vec<(&'static str, Box<dyn HistoryProvider>)> {
    vec![
        (
            "claude",
            Box::new(ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")])),
        ),
        (
            "copilot",
            Box::new(CopilotCliProvider::new(
                vec![fixtures_dir().join("copilot")],
            )),
        ),
        (
            "copilot_v2",
            Box::new(CopilotCliProvider::new(vec![
                fixtures_dir().join("copilot_v2")
            ])),
        ),
        (
            "codex",
            Box::new(CodexCliProvider::new(vec![fixtures_dir().join("codex")])),
        ),
        (
            "codex_v2",
            Box::new(CodexCliProvider::new(vec![fixtures_dir().join("codex_v2")])),
        ),
        (
            "opencode",
            Box::new(OpenCodeProvider::new(vec![fixtures_dir().join("opencode")])),
        ),
        (
            "opencode_v2",
            Box::new(OpenCodeProvider::new(vec![
                fixtures_dir().join("opencode_v2")
            ])),
        ),
        (
            "gemini",
            Box::new(GeminiCliProvider::new(vec![fixtures_dir().join("gemini")])),
        ),
    ]
}

fn diagnose_all_fixtures() -> Vec<ProviderDiagnostic> {
    fixture_provider_set()
        .into_iter()
        .map(|(label, p)| {
            analyze_provider(label, p.as_ref(), None)
                .unwrap_or_else(|e| panic!("{label}: analyze_provider failed: {e}"))
        })
        .collect()
}
