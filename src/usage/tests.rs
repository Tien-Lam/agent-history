use std::path::PathBuf;

use chrono::{TimeZone, Utc};

use super::*;
use crate::model::SessionId;

fn ts(secs: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

fn mk_session(
    id: &str,
    provider: Provider,
    model: Option<&str>,
    project: Option<&str>,
    usage: Option<TokenUsage>,
) -> Session {
    Session {
        id: SessionId(id.to_string()),
        provider,
        project_path: project.map(PathBuf::from),
        project_name: project.map(str::to_string),
        git_branch: None,
        started_at: ts(0),
        ended_at: None,
        summary: None,
        model: model.map(str::to_string),
        token_usage: usage,
        message_count: 1,
        source_path: PathBuf::from(format!("/tmp/{id}")),
    }
}

#[test]
fn pricing_for_known_prefix_returns_rate() {
    let p = pricing_for("claude-sonnet-4-5-20250929").unwrap();
    assert!((p.input_per_mtok - 3.0).abs() < f64::EPSILON);
    assert!((p.output_per_mtok - 15.0).abs() < f64::EPSILON);
}

#[test]
fn pricing_for_longer_prefix_wins() {
    // Both "claude-3-5-sonnet" and "claude-3-haiku" share "claude-3-",
    // but neither is a prefix of "claude-3-5-sonnet-20240620". The
    // longest matching prefix should be the 3-5-sonnet entry.
    let p = pricing_for("claude-3-5-sonnet-20240620").unwrap();
    assert!((p.input_per_mtok - 3.0).abs() < f64::EPSILON);
}

#[test]
fn pricing_for_unknown_returns_none() {
    assert!(pricing_for("totally-imaginary-model").is_none());
    assert!(pricing_for("").is_none());
}

#[test]
fn cost_usd_includes_input_output_and_cache() {
    let p = pricing_for("claude-sonnet-4-5").unwrap();
    let usage = TokenUsage {
        input_tokens: 1_000_000,
        output_tokens: 1_000_000,
        cache_read_tokens: Some(1_000_000),
        cache_write_tokens: Some(1_000_000),
    };
    // 3 + 15 + 0.3 + 3.75 = 22.05
    let cost = p.cost_usd(&usage);
    assert!((cost - 22.05).abs() < 1e-9, "got {cost}");
}

#[test]
fn cost_usd_skips_cache_when_no_rate_available() {
    let p = pricing_for("gpt-4-turbo").unwrap();
    let usage = TokenUsage {
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: Some(1_000_000),
        cache_write_tokens: Some(1_000_000),
    };
    // GPT-4 Turbo entry has no cache rates, so the cache tokens
    // contribute zero rather than being valued at the input rate.
    assert!(p.cost_usd(&usage).abs() < f64::EPSILON);
}

#[test]
fn aggregate_groups_by_model_and_sums_tokens() {
    let usage_a = TokenUsage {
        input_tokens: 1_000,
        output_tokens: 500,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    let usage_b = TokenUsage {
        input_tokens: 200,
        output_tokens: 100,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    let sessions = vec![
        mk_session(
            "a",
            Provider::ClaudeCode,
            Some("claude-sonnet-4-5"),
            Some("foo"),
            Some(usage_a),
        ),
        mk_session(
            "b",
            Provider::ClaudeCode,
            Some("claude-sonnet-4-5"),
            Some("foo"),
            Some(usage_b),
        ),
    ];
    let report = aggregate(&sessions, GroupBy::Model);
    assert_eq!(report.rows.len(), 1);
    let row = &report.rows[0];
    assert_eq!(row.key, "claude-sonnet-4-5");
    assert_eq!(row.session_count, 2);
    assert_eq!(row.input_tokens, 1_200);
    assert_eq!(row.output_tokens, 600);
    assert_eq!(row.total_tokens, 1_800);
    // 0.0012 * 3 + 0.0006 * 15 = 0.0036 + 0.009 = 0.0126
    assert!((row.cost_usd.unwrap() - 0.0126).abs() < 1e-9);
    assert_eq!(report.totals.session_count, 2);
    assert!((report.totals.cost_usd.unwrap() - 0.0126).abs() < 1e-9);
}

#[test]
fn aggregate_unknown_model_keeps_tokens_drops_cost() {
    let usage = TokenUsage {
        input_tokens: 1_000,
        output_tokens: 500,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    let sessions = vec![mk_session(
        "a",
        Provider::ClaudeCode,
        Some("future-model-7"),
        None,
        Some(usage),
    )];
    let report = aggregate(&sessions, GroupBy::Model);
    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].input_tokens, 1_000);
    assert!(report.rows[0].cost_usd.is_none());
    assert!(report.totals.cost_usd.is_none());
}

#[test]
fn aggregate_priced_plus_unpriced_drops_cost_for_overall_only() {
    // Bucket A is fully priced; bucket B is unpriced. Each row's
    // own cost stands; only the overall total goes to None.
    let usage = TokenUsage {
        input_tokens: 1_000,
        output_tokens: 500,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    let sessions = vec![
        mk_session(
            "a",
            Provider::ClaudeCode,
            Some("claude-sonnet-4-5"),
            None,
            Some(usage.clone()),
        ),
        mk_session(
            "b",
            Provider::CodexCli,
            Some("future-model-7"),
            None,
            Some(usage),
        ),
    ];
    let report = aggregate(&sessions, GroupBy::Model);
    assert_eq!(report.rows.len(), 2);
    let priced = report
        .rows
        .iter()
        .find(|r| r.key == "claude-sonnet-4-5")
        .unwrap();
    assert!(priced.cost_usd.is_some());
    let unpriced = report
        .rows
        .iter()
        .find(|r| r.key == "future-model-7")
        .unwrap();
    assert!(unpriced.cost_usd.is_none());
    assert!(report.totals.cost_usd.is_none());
}

#[test]
fn aggregate_missing_model_uses_unknown_bucket() {
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    let sessions = vec![mk_session(
        "a",
        Provider::ClaudeCode,
        None,
        None,
        Some(usage),
    )];
    let report = aggregate(&sessions, GroupBy::Model);
    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].key, "(unknown)");
    assert!(report.rows[0].cost_usd.is_none());
}

#[test]
fn aggregate_by_provider_groups_across_models() {
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    let sessions = vec![
        mk_session(
            "a",
            Provider::ClaudeCode,
            Some("claude-sonnet-4-5"),
            None,
            Some(usage.clone()),
        ),
        mk_session(
            "b",
            Provider::ClaudeCode,
            Some("claude-haiku-4-5"),
            None,
            Some(usage),
        ),
    ];
    let report = aggregate(&sessions, GroupBy::Provider);
    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].key, "claude-code");
    assert_eq!(report.rows[0].session_count, 2);
}

#[test]
fn aggregate_by_project_normalizes_missing_to_unknown() {
    let usage = TokenUsage {
        input_tokens: 10,
        output_tokens: 5,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    let sessions = vec![
        mk_session(
            "a",
            Provider::ClaudeCode,
            Some("claude-sonnet-4-5"),
            Some("aghist"),
            Some(usage.clone()),
        ),
        mk_session(
            "b",
            Provider::ClaudeCode,
            Some("claude-sonnet-4-5"),
            None,
            Some(usage),
        ),
    ];
    let report = aggregate(&sessions, GroupBy::Project);
    let keys: Vec<&str> = report.rows.iter().map(|r| r.key.as_str()).collect();
    assert!(keys.contains(&"aghist"));
    assert!(keys.contains(&"(unknown)"));
}

#[test]
fn aggregate_sessions_without_usage_count_but_dont_inflate_tokens() {
    let sessions = vec![mk_session(
        "a",
        Provider::ClaudeCode,
        Some("claude-sonnet-4-5"),
        None,
        None,
    )];
    let report = aggregate(&sessions, GroupBy::Model);
    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].session_count, 1);
    assert_eq!(report.rows[0].input_tokens, 0);
    assert_eq!(report.rows[0].output_tokens, 0);
    assert_eq!(report.rows[0].total_tokens, 0);
    assert_eq!(report.rows[0].cost_usd, Some(0.0));
}

#[test]
fn rows_sorted_by_total_tokens_descending() {
    let big = TokenUsage {
        input_tokens: 10_000,
        output_tokens: 1_000,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    let small = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    let sessions = vec![
        mk_session(
            "a",
            Provider::ClaudeCode,
            Some("claude-haiku-4-5"),
            None,
            Some(small),
        ),
        mk_session(
            "b",
            Provider::ClaudeCode,
            Some("claude-sonnet-4-5"),
            None,
            Some(big),
        ),
    ];
    let report = aggregate(&sessions, GroupBy::Model);
    assert_eq!(report.rows[0].key, "claude-sonnet-4-5");
    assert_eq!(report.rows[1].key, "claude-haiku-4-5");
}

#[test]
fn group_by_parse_round_trips() {
    for s in ["model", "provider", "project"] {
        let g = GroupBy::parse(s).unwrap();
        assert_eq!(g.as_str(), s);
    }
    assert!(GroupBy::parse("session").is_err());
}
