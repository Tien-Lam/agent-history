use super::*;

#[test]
fn matched_projects_dedupes_and_sorts() {
    let s1 = mk_session(
        "s1",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        0,
    );
    let s2 = mk_session(
        "s2",
        Some("alpha-deep-dive"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 10),
        0,
    );
    let s3 = mk_session(
        "s3",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 11),
        0,
    );
    let report = aggregate(
        "alpha",
        &[(s1, vec![]), (s2, vec![]), (s3, vec![])],
        ProjectLimits::DEFAULTS,
    );
    assert_eq!(
        report.matched_projects,
        vec!["alpha".to_string(), "alpha-deep-dive".to_string()]
    );
}

#[test]
fn empty_input_produces_zero_envelope() {
    let report = aggregate("ghost", &[], ProjectLimits::DEFAULTS);
    assert_eq!(report.session_count, 0);
    assert_eq!(report.message_count, 0);
    assert_eq!(report.token_usage.total_tokens, 0);
    assert_eq!(report.token_usage.cost_usd, Some(0.0));
    assert!(report.decisions.is_empty());
    assert!(report.todos.is_empty());
    assert!(report.threads.is_empty());
    assert!(report.top_files.is_empty());
    assert!(report.started_at.is_none());
    assert!(report.ended_at.is_none());
    assert!(report.matched_projects.is_empty());
    assert_eq!(report.time_of_day.iter().sum::<u64>(), 0);
}
