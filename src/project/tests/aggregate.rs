use super::*;

#[test]
fn aggregate_counts_sessions_messages_and_token_totals() {
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_read_tokens: Some(10),
        cache_write_tokens: Some(20),
    };
    let s1 = mk_session(
        "s1",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        Some(usage.clone()),
        ts(2025, 9),
        2,
    );
    let m1 = vec![
        assistant("first message in alpha", ts(2025, 9)),
        assistant("second message", ts(2025, 10)),
    ];
    let s2 = mk_session(
        "s2",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        Some(usage),
        ts(2025, 14),
        1,
    );
    let m2 = vec![assistant("late afternoon", ts(2025, 14))];

    let report = aggregate("alpha", &[(s1, m1), (s2, m2)], ProjectLimits::DEFAULTS);

    assert_eq!(report.query, "alpha");
    assert_eq!(report.matched_projects, vec!["alpha".to_string()]);
    assert_eq!(report.session_count, 2);
    assert_eq!(report.message_count, 3);
    assert_eq!(report.token_usage.input_tokens, 200);
    assert_eq!(report.token_usage.output_tokens, 100);
    assert_eq!(report.token_usage.cache_read_tokens, 20);
    assert_eq!(report.token_usage.cache_write_tokens, 40);
    assert_eq!(report.token_usage.total_tokens, 360);
    assert!(report.token_usage.cost_usd.is_some());
    assert_eq!(report.time_of_day[9], 1);
    assert_eq!(report.time_of_day[10], 1);
    assert_eq!(report.time_of_day[14], 1);
    assert!(report.started_at.is_some());
    assert!(report.ended_at.is_some());
}

#[test]
fn aggregate_with_unpriced_model_nulls_cost_but_keeps_tokens() {
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_read_tokens: None,
        cache_write_tokens: None,
    };
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("future-model-7"),
        Some(usage),
        ts(2025, 9),
        1,
    );
    let m = vec![assistant("hi", ts(2025, 9))];
    let report = aggregate("alpha", &[(s, m)], ProjectLimits::DEFAULTS);
    assert_eq!(report.token_usage.input_tokens, 100);
    assert!(report.token_usage.cost_usd.is_none());
}

#[test]
fn time_of_day_uses_message_timestamps_in_utc() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        2,
    );
    let m = vec![
        assistant("morning", ts(2025, 9)),
        assistant("evening", ts(2025, 21)),
    ];
    let report = aggregate("alpha", &[(s, m)], ProjectLimits::DEFAULTS);
    assert_eq!(report.time_of_day[9], 1);
    assert_eq!(report.time_of_day[21], 1);
    let total: u64 = report.time_of_day.iter().sum();
    assert_eq!(total, 2);
}
