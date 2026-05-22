use super::diagnose_all_fixtures;

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
