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
mod tests {
    use super::*;
    use crate::provider::ProviderParseStats;

    #[test]
    fn provider_parse_health_check_reports_clean_sample() {
        let diag = diagnostic_with_parse(ProviderParseStats {
            records_seen: 2,
            parse_errors: 0,
            skipped_records: 0,
            empty_content: 0,
        });

        let check = provider_parse_health_check(&[diag]).expect("parse health check");

        assert_eq!(check.name, "provider-parse-warnings");
        assert_eq!(check.status, HealthStatus::Ok);
        assert!(check.hint.is_none());
    }

    #[test]
    fn provider_parse_health_check_warns_with_actionable_counts() {
        let diag = diagnostic_with_parse(ProviderParseStats {
            records_seen: 4,
            parse_errors: 1,
            skipped_records: 2,
            empty_content: 1,
        });

        let check = provider_parse_health_check(&[diag]).expect("parse health check");

        assert_eq!(check.name, "provider-parse-warnings");
        assert_eq!(check.status, HealthStatus::Warn);
        assert!(check.message.contains("claude-code"));
        assert!(check.message.contains("records=4"));
        assert!(check.message.contains("parse_errors=1"));
        assert!(check.message.contains("skipped=2"));
        assert!(check.message.contains("empty=1"));
        assert!(check.hint.as_deref().unwrap().contains("format drift"));
    }

    fn diagnostic_with_parse(parse: ProviderParseStats) -> ProviderDiagnostic {
        ProviderDiagnostic {
            label: "claude-code".to_string(),
            provider: "claude-code".to_string(),
            session_count: 1,
            message_count: 1,
            parse,
            blocks: crate::provider_diagnostic::BlockCounts::default(),
            tool_call_fidelity: crate::provider_diagnostic::ToolCallFidelity::default(),
        }
    }
}
