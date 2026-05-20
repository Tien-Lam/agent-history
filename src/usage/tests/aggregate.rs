use super::*;

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
