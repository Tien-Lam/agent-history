use super::diagnose_all_fixtures;

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
