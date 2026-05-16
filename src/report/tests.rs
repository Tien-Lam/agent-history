use chrono::{DateTime, Duration, TimeZone, Utc};
use std::path::PathBuf;

use super::*;
use crate::model::{ContentBlock, MessageId, Provider, Role, SessionId, TokenUsage};

fn ts(year: i32, day: u32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, 1, day, hour, 0, 0).unwrap()
}

fn mk_session(
    id: &str,
    project: Option<&str>,
    model: Option<&str>,
    usage: Option<TokenUsage>,
    started: DateTime<Utc>,
    message_count: usize,
) -> Session {
    Session {
        id: SessionId(id.to_string()),
        provider: Provider::ClaudeCode,
        project_path: project.map(PathBuf::from),
        project_name: project.map(str::to_string),
        git_branch: None,
        started_at: started,
        ended_at: Some(started + chrono::Duration::minutes(5)),
        summary: None,
        model: model.map(str::to_string),
        token_usage: usage,
        message_count,
        source_path: PathBuf::from(format!("/tmp/{id}")),
    }
}

fn assistant(text: &str, when: DateTime<Utc>) -> Message {
    Message {
        id: MessageId(format!("m-{}", when.timestamp())),
        role: Role::Assistant,
        timestamp: when,
        content: vec![ContentBlock::Text(text.into())],
        model: None,
        token_usage: None,
    }
}

fn priced_usage() -> TokenUsage {
    TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_read_tokens: Some(10),
        cache_write_tokens: Some(20),
    }
}

#[test]
fn window_last_days_clamps_to_one() {
    let end = ts(2026, 5, 10);
    let w = ReportWindow::last_days(end, 0);
    assert_eq!(w.days, 1);
    assert_eq!(w.ended_at, end);
    assert_eq!(w.started_at, end - Duration::days(1));
}

#[test]
fn window_between_rounds_up_partial_days() {
    let start = ts(2026, 5, 10);
    let end = start + Duration::hours(6);
    assert_eq!(ReportWindow::between(start, end).days, 1);
    let end = start + Duration::days(7);
    assert_eq!(ReportWindow::between(start, end).days, 7);
}

#[test]
fn aggregate_counts_sessions_messages_and_tokens_across_projects() {
    let s1 = mk_session(
        "s1",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        Some(priced_usage()),
        ts(2026, 5, 9),
        2,
    );
    let m1 = vec![
        assistant("first message in alpha", ts(2026, 5, 9)),
        assistant("second", ts(2026, 5, 9)),
    ];
    let s2 = mk_session(
        "s2",
        Some("beta"),
        Some("claude-sonnet-4-5"),
        Some(priced_usage()),
        ts(2026, 5, 10),
        1,
    );
    let m2 = vec![assistant("noise in beta", ts(2026, 5, 10))];

    let window = ReportWindow::last_days(ts(2026, 5, 10), 7);
    let env = aggregate(window, &[(s1, m1), (s2, m2)], ReportLimits::DEFAULTS);

    assert_eq!(env.session_count, 2);
    assert_eq!(env.message_count, 3);
    assert_eq!(env.project_count, 2);
    assert_eq!(env.meta.projects_total, 2);
    assert_eq!(env.token_usage.input_tokens, 200);
    assert_eq!(env.token_usage.output_tokens, 100);
    assert_eq!(env.token_usage.total_tokens, 360);
    assert!(env.token_usage.cost_usd.is_some());
    assert_eq!(env.top_projects[0].project, "alpha");
    assert_eq!(env.top_projects[0].session_count, 1);
    assert_eq!(env.top_projects[0].message_count, 2);
    assert_eq!(env.top_projects[1].project, "beta");
}

#[test]
fn top_projects_truncates_but_meta_preserves_total() {
    let mut bundles = Vec::new();
    for (i, name) in ["alpha", "beta", "gamma", "delta"].iter().enumerate() {
        let hour = u32::try_from(9 + i).expect("test indices fit in u32");
        let s = mk_session(
            &format!("s{i}"),
            Some(*name),
            Some("claude-sonnet-4-5"),
            None,
            ts(2026, 5, hour),
            i + 1,
        );
        // Different message counts so ranking is deterministic.
        let msgs: Vec<Message> = (0..=i)
            .map(|j| assistant(&format!("msg {j}"), ts(2026, 5, hour)))
            .collect();
        bundles.push((s, msgs));
    }
    let window = ReportWindow::last_days(ts(2026, 5, 13), 7);
    let limits = ReportLimits {
        top_projects: 2,
        decisions: 5,
        todos: 5,
        threads: 5,
    };
    let env = aggregate(window, &bundles, limits);
    assert_eq!(env.top_projects.len(), 2);
    assert_eq!(env.meta.projects_total, 4);
    assert_eq!(env.top_projects[0].project, "delta");
    assert_eq!(env.top_projects[1].project, "gamma");
}

#[test]
fn unknown_project_bucket_used_for_missing_project_name() {
    let s = mk_session(
        "s",
        None,
        Some("claude-sonnet-4-5"),
        None,
        ts(2026, 5, 10),
        1,
    );
    let m = vec![assistant("hello", ts(2026, 5, 10))];
    let window = ReportWindow::last_days(ts(2026, 5, 10), 7);
    let env = aggregate(window, &[(s, m)], ReportLimits::DEFAULTS);
    assert_eq!(env.top_projects.len(), 1);
    assert_eq!(env.top_projects[0].project, "(unknown)");
}

#[test]
fn unpriced_model_nulls_envelope_cost_but_keeps_tokens() {
    let usage = priced_usage();
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("future-model-7"),
        Some(usage),
        ts(2026, 5, 10),
        1,
    );
    let m = vec![assistant("hi", ts(2026, 5, 10))];
    let window = ReportWindow::last_days(ts(2026, 5, 10), 7);
    let env = aggregate(window, &[(s, m)], ReportLimits::DEFAULTS);
    assert_eq!(env.token_usage.input_tokens, 100);
    assert!(env.token_usage.cost_usd.is_none());
    assert!(env.top_projects[0].cost_usd.is_none());
}

#[test]
fn decisions_extracted_and_sorted_by_score() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2026, 5, 9),
        2,
    );
    let m = vec![
        assistant("Background context.", ts(2026, 5, 9)),
        assistant("We decided to use BM25 instead of cosine.", ts(2026, 5, 9)),
    ];
    let window = ReportWindow::last_days(ts(2026, 5, 10), 7);
    let env = aggregate(window, &[(s, m)], ReportLimits::DEFAULTS);
    assert!(!env.decisions.is_empty());
    assert!(env.decisions[0].reference.contains("claude-code/s#"));
}

#[test]
fn todos_extracted_with_citation_refs() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2026, 5, 10),
        1,
    );
    let m = vec![assistant("TODO: revisit error handling", ts(2026, 5, 10))];
    let window = ReportWindow::last_days(ts(2026, 5, 10), 7);
    let env = aggregate(window, &[(s, m)], ReportLimits::DEFAULTS);
    assert!(!env.todos.is_empty());
    assert!(env.todos[0].reference.contains("#1"));
}

#[test]
fn empty_input_produces_zero_envelope() {
    let window = ReportWindow::last_days(ts(2026, 5, 10), 7);
    let env = aggregate(window, &[], ReportLimits::DEFAULTS);
    assert_eq!(env.session_count, 0);
    assert_eq!(env.message_count, 0);
    assert_eq!(env.project_count, 0);
    assert_eq!(env.token_usage.total_tokens, 0);
    assert_eq!(env.token_usage.cost_usd, Some(0.0));
    assert!(env.top_projects.is_empty());
    assert!(env.decisions.is_empty());
    assert!(env.todos.is_empty());
    assert!(env.threads.is_empty());
}

#[test]
fn render_markdown_includes_window_and_section_headers() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        Some(priced_usage()),
        ts(2026, 5, 10),
        1,
    );
    let m = vec![assistant("we decided to ship", ts(2026, 5, 10))];
    let window = ReportWindow::last_days(ts(2026, 5, 10), 7);
    let env = aggregate(window, &[(s, m)], ReportLimits::DEFAULTS);
    let md = render_markdown(&env);
    assert!(md.contains("# Weekly summary"));
    assert!(md.contains("(7 days)"));
    assert!(md.contains("## Top projects"));
    assert!(md.contains("**alpha**"));
    assert!(md.contains("## Decisions"));
    assert!(md.contains("## Open TODOs"));
    assert!(md.contains("## Threads"));
}

#[test]
fn render_markdown_uses_singular_day_for_one_day_window() {
    let window = ReportWindow::last_days(ts(2026, 5, 10), 1);
    let env = aggregate(window, &[], ReportLimits::DEFAULTS);
    let md = render_markdown(&env);
    assert!(
        md.contains("(1 day)"),
        "expected singular `1 day`, got: {md}"
    );
}

#[test]
fn render_markdown_handles_empty_envelope_gracefully() {
    let window = ReportWindow::last_days(ts(2026, 5, 10), 7);
    let env = aggregate(window, &[], ReportLimits::DEFAULTS);
    let md = render_markdown(&env);
    assert!(md.contains("_No project activity"));
    assert!(md.contains("_No decision candidates"));
    assert!(md.contains("_No open TODOs"));
    assert!(md.contains("_No threads"));
}
