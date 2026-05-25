use super::diagnose_all_fixtures;
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;
use std::collections::BTreeMap;

#[test]
fn generated_provider_diagnostics_cover_every_registered_provider() {
    let (_dirs, providers) = super::common::fixtures::generated::all_generated_providers(1, 2);

    let mut actual = providers
        .iter()
        .map(|provider| provider.provider())
        .collect::<Vec<_>>();
    actual.sort_by_key(|provider| provider.slug());

    let mut expected = aghist::model::Provider::all().to_vec();
    expected.sort_by_key(|provider| provider.slug());

    assert_eq!(
        actual, expected,
        "generated fixture provider matrix drifted away from registered providers",
    );

    for provider in providers {
        let label = provider.provider().slug();
        let diagnostic =
            aghist::provider_diagnostic::analyze_provider(label, provider.as_ref(), None)
                .unwrap_or_else(|err| panic!("{label}: analyze_provider failed: {err}"));

        assert_eq!(
            diagnostic.session_count, 1,
            "{label}: generated diagnostics should keep one session per provider",
        );
        assert_eq!(
            diagnostic.message_count, 2,
            "{label}: generated diagnostics should keep two messages per provider",
        );
        assert!(
            diagnostic.blocks.total >= 2,
            "{label}: generated diagnostics should produce message blocks",
        );
        assert!(
            diagnostic.parse.records_seen >= diagnostic.message_count,
            "{label}: generated diagnostics should account for every parsed message record",
        );
    }
}

#[test]
fn all_fixture_providers_tool_call_fidelity() {
    let diagnostics = diagnose_all_fixtures();

    for diag in &diagnostics {
        let f = &diag.tool_call_fidelity;
        let label = &diag.label;

        assert_eq!(
            f.empty_names, 0,
            "[{label}] {} ToolUse block(s) had an empty name - name is the primary search key",
            f.empty_names,
        );
        assert_eq!(
            f.invalid_json_args, 0,
            "[{label}] {} ToolUse block(s) had non-empty arguments that failed to parse as JSON",
            f.invalid_json_args,
        );
    }

    let total_tool_calls: usize = diagnostics
        .iter()
        .map(|d| d.tool_call_fidelity.tool_calls)
        .sum();
    assert!(
        total_tool_calls > 0,
        "no fixture produced any ToolUse blocks - tool-call extraction may be silently broken across all providers",
    );
}

#[test]
fn edge_case_provider_diagnostics_do_not_abort_on_malformed_fixtures() {
    let root = super::common::helpers::edge_cases_dir();
    let cases: Vec<(&str, Box<dyn HistoryProvider>)> = vec![
        (
            "claude-edge",
            Box::new(ClaudeCodeProvider::new(vec![root.join("claude")])),
        ),
        (
            "codex-edge",
            Box::new(CodexCliProvider::new(vec![root.join("codex")])),
        ),
        (
            "copilot-edge",
            Box::new(CopilotCliProvider::new(vec![root.join("copilot")])),
        ),
        (
            "gemini-edge",
            Box::new(GeminiCliProvider::new(vec![root.join("gemini")])),
        ),
        (
            "opencode-edge",
            Box::new(OpenCodeProvider::new(vec![root.join("opencode")])),
        ),
    ];

    let diagnostics = cases
        .into_iter()
        .map(|(label, provider)| {
            aghist::provider_diagnostic::analyze_provider(label, provider.as_ref(), None)
                .unwrap_or_else(|err| panic!("{label}: edge diagnostic failed: {err}"))
        })
        .map(|diagnostic| (diagnostic.label.clone(), diagnostic))
        .collect::<BTreeMap<_, _>>();

    let claude = &diagnostics["claude-edge"];
    assert_eq!(claude.session_count, 1);
    assert_eq!(claude.message_count, 2);
    assert_eq!(claude.parse.records_seen, 5);
    assert_eq!(claude.parse.parse_errors, 2);
    assert_eq!(claude.parse.empty_content, 1);

    let codex = &diagnostics["codex-edge"];
    assert_eq!(codex.session_count, 1);
    assert_eq!(codex.message_count, 2);
    assert_eq!(codex.parse.records_seen, 4);
    assert_eq!(codex.parse.parse_errors, 2);

    assert_eq!(diagnostics["copilot-edge"].session_count, 1);
    assert_eq!(diagnostics["copilot-edge"].message_count, 0);
    assert_eq!(diagnostics["gemini-edge"].session_count, 0);
    assert_eq!(diagnostics["opencode-edge"].session_count, 1);
}

#[test]
fn all_fixture_providers_diagnostic_summary_snapshot() {
    let diagnostics = diagnose_all_fixtures();
    let pretty = serde_json::to_string_pretty(&diagnostics)
        .expect("ProviderDiagnostic must serialise as JSON");
    insta::with_settings!({ snapshot_path => "../snapshots", prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!(
            "provider_format_diagnostic__all_fixture_providers_diagnostic_summary_snapshot",
            pretty
        );
    });
}
