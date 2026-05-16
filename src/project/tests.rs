use chrono::{DateTime, TimeZone, Utc};
use std::path::PathBuf;

use super::*;
use crate::model::{
    ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, TokenUsage, ToolCall,
};

fn ts(year: i32, hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, 1, 1, hour, 0, 0).unwrap()
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

fn tool_call(name: &str, args: &str, when: DateTime<Utc>) -> Message {
    Message {
        id: MessageId(format!("t-{}", when.timestamp())),
        role: Role::Assistant,
        timestamp: when,
        content: vec![ContentBlock::ToolUse(ToolCall {
            id: "call-1".into(),
            name: name.into(),
            arguments: args.into(),
        })],
        model: None,
        token_usage: None,
    }
}

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
fn decisions_extracted_and_sorted_by_score() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        2,
    );
    let m = vec![
        assistant("Background context.", ts(2025, 9)),
        assistant("We decided to use BM25 instead of cosine.", ts(2025, 10)),
    ];
    let report = aggregate("alpha", &[(s, m)], ProjectLimits::DEFAULTS);
    assert!(
        !report.decisions.is_empty(),
        "expected at least one decision"
    );
    let d = &report.decisions[0];
    assert_eq!(d.turn, 2);
    assert_eq!(d.session_id, "s");
    assert!(d.reference.contains("claude-code/s#2"));
    assert!(d.score >= 5.0);
}

#[test]
fn todos_extracted_with_citation_refs() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        1,
    );
    let m = vec![assistant("TODO: revisit error handling", ts(2025, 9))];
    let report = aggregate("alpha", &[(s, m)], ProjectLimits::DEFAULTS);
    assert!(!report.todos.is_empty());
    assert_eq!(report.todos[0].turn, 1);
    assert!(report.todos[0].reference.contains("#1"));
}

#[test]
fn top_files_counts_tool_call_paths() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        3,
    );
    let m = vec![
        tool_call("Read", r#"{"file_path":"src/main.rs"}"#, ts(2025, 9)),
        tool_call("Edit", r#"{"file_path":"src/main.rs"}"#, ts(2025, 10)),
        tool_call("Read", r#"{"file_path":"src/lib.rs"}"#, ts(2025, 11)),
    ];
    let report = aggregate("alpha", &[(s, m)], ProjectLimits::DEFAULTS);
    assert_eq!(report.top_files.len(), 2);
    assert_eq!(report.top_files[0].path, "src/main.rs");
    assert_eq!(report.top_files[0].count, 2);
    assert_eq!(report.top_files[1].path, "src/lib.rs");
    assert_eq!(report.top_files[1].count, 1);
    assert_eq!(report.meta.files_total, 2);
}

#[test]
fn top_files_skips_non_path_args_and_unparseable_json() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        3,
    );
    let m = vec![
        tool_call("Bash", r#"{"command":"ls -la"}"#, ts(2025, 9)),
        tool_call("Weird", "not-valid-json", ts(2025, 10)),
        tool_call("Read", r#"{"file_path":""}"#, ts(2025, 11)),
    ];
    let report = aggregate("alpha", &[(s, m)], ProjectLimits::DEFAULTS);
    assert!(report.top_files.is_empty());
}

#[test]
fn limits_truncate_sections_but_meta_preserves_totals() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        3,
    );
    let m = vec![
        tool_call("Read", r#"{"file_path":"a"}"#, ts(2025, 9)),
        tool_call("Read", r#"{"file_path":"b"}"#, ts(2025, 10)),
        tool_call("Read", r#"{"file_path":"c"}"#, ts(2025, 11)),
    ];
    let limits = ProjectLimits {
        decisions: 5,
        todos: 5,
        threads: 5,
        files: 2,
    };
    let report = aggregate("alpha", &[(s, m)], limits);
    assert_eq!(report.top_files.len(), 2);
    assert_eq!(report.meta.files_total, 3);
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
    // Cost is reported as Some(0.0) because there were no unpriced sessions.
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
