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
