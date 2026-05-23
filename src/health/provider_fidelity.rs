use crate::health::{HealthCheck, HealthStatus};
use crate::provider::HistoryProvider;
use crate::provider_diagnostic::{analyze_provider, ProviderDiagnostic};

/// Cap on sessions sampled per provider when computing the live-data
/// fidelity summary. Real session stores can hold thousands of sessions;
/// `aghist health` must stay snappy, so we sample the most recent
/// `discover_sessions` returned (provider-specific ordering).
pub const HEALTH_FIDELITY_SAMPLE_PER_PROVIDER: usize = 5;

/// Sample each detected provider for tool-call fidelity. Returns one
/// [`ProviderDiagnostic`] per provider. Errors during discover/load are
/// converted to a synthetic diagnostic with `provider` set to the slug
/// and zero counts so the caller's output remains a stable list.
///
/// Capped at [`HEALTH_FIDELITY_SAMPLE_PER_PROVIDER`] sessions per provider
/// so this never blocks `aghist health` on huge real-world stores.
#[must_use]
pub fn run_provider_fidelity(providers: &[Box<dyn HistoryProvider>]) -> Vec<ProviderDiagnostic> {
    providers
        .iter()
        .map(|p| {
            let slug = p.provider().slug();
            analyze_provider(slug, p.as_ref(), Some(HEALTH_FIDELITY_SAMPLE_PER_PROVIDER))
                .unwrap_or_else(|_| ProviderDiagnostic {
                    label: slug.to_string(),
                    provider: slug.to_string(),
                    session_count: 0,
                    message_count: 0,
                    parse: crate::provider::ProviderParseStats::default(),
                    blocks: crate::provider_diagnostic::BlockCounts::default(),
                    tool_call_fidelity: crate::provider_diagnostic::ToolCallFidelity::default(),
                })
        })
        .collect()
}

#[must_use]
pub fn provider_parse_health_check(fidelity: &[ProviderDiagnostic]) -> Option<HealthCheck> {
    if fidelity.is_empty() {
        return None;
    }

    let warnings: Vec<String> = fidelity
        .iter()
        .filter(|d| d.parse.has_warnings())
        .map(provider_parse_summary)
        .collect();

    if warnings.is_empty() {
        return Some(HealthCheck {
            name: "provider-parse-warnings",
            status: HealthStatus::Ok,
            message: "sampled provider records parsed without warnings".to_string(),
            hint: None,
        });
    }

    Some(HealthCheck {
        name: "provider-parse-warnings",
        status: HealthStatus::Warn,
        message: format!(
            "sampled provider records had parse warnings: {}",
            warnings.join("; ")
        ),
        hint: Some(
            "These usually mean provider format drift or corrupt history; inspect the affected provider files and update the parser or remove stale records."
                .to_string(),
        ),
    })
}

fn provider_parse_summary(diag: &ProviderDiagnostic) -> String {
    let parse = &diag.parse;
    format!(
        "{} records={} parse_errors={} skipped={} empty={}",
        diag.provider,
        parse.records_seen,
        parse.parse_errors,
        parse.skipped_records,
        parse.empty_content
    )
}

#[cfg(test)]
mod tests;
