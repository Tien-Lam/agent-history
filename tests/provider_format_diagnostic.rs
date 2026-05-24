//! Diagnostic tests for real-world provider format compatibility.
//!
//! These tests use fixtures that match actual formats found on real systems.

mod common;

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

fn diagnose_all_fixtures() -> Vec<ProviderDiagnostic> {
    common::provider_conformance::cases::static_fixture_provider_cases()
        .into_iter()
        .map(|case| {
            analyze_provider(case.label, case.provider.as_ref(), None)
                .unwrap_or_else(|e| panic!("{}: analyze_provider failed: {e}", case.label))
        })
        .collect()
}
