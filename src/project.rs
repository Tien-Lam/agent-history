//! Per-project productivity dashboard. Powers `aghist project <name>`.
//!
//! Aggregates one project's history into a single envelope: session/message
//! counts, token usage (with cost when known), heuristic decisions/todos,
//! cross-session threads, the files most often touched by tool calls, and a
//! 24-bucket UTC time-of-day histogram.
//!
//! All sub-aggregations reuse existing extractors ([`crate::decisions`],
//! [`crate::todos`], [`crate::threads`], [`crate::usage::pricing_for`]) so the
//! shapes stay consistent with the standalone subcommands. There's no LLM
//! involved — agents that want richer interpretation can post-process by
//! `aghist show`-ing the citation refs.

use chrono::{DateTime, Timelike, Utc};
use serde::Serialize;

use crate::decisions::DEFAULT_THRESHOLD as DECISIONS_THRESHOLD;
use crate::model::{Message, Session};
use crate::threads::{self, ClusterOptions, Thread, DEFAULT_GAP_HOURS};

mod extract;
mod files;
mod tokens;

pub use extract::{collect_decisions, collect_todos, DecisionRow, TodoRow};
use files::top_files;
pub use files::FileTouch;
pub use tokens::{aggregate_tokens, ProjectTokens};

/// Per-section caps. Zero means "no cap" for the corresponding section.
#[derive(Debug, Clone, Copy)]
pub struct ProjectLimits {
    pub decisions: usize,
    pub todos: usize,
    pub threads: usize,
    pub files: usize,
}

impl ProjectLimits {
    /// Defaults sized for a quick TTY scan: a handful of headlines per
    /// section, not an exhaustive dump.
    pub const DEFAULTS: Self = Self {
        decisions: 5,
        todos: 10,
        threads: 5,
        files: 10,
    };
}

impl Default for ProjectLimits {
    fn default() -> Self {
        Self::DEFAULTS
    }
}

/// The per-project dashboard envelope. Section caps are applied during
/// aggregation; raw counts live in [`ProjectMeta`] so callers can tell
/// "5 of 23" from "5 of 5".
#[derive(Debug, Clone, Serialize)]
pub struct ProjectReport {
    pub query: String,
    pub matched_projects: Vec<String>,
    pub session_count: usize,
    pub message_count: usize,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub token_usage: ProjectTokens,
    pub decisions: Vec<DecisionRow>,
    pub todos: Vec<TodoRow>,
    pub threads: Vec<Thread>,
    pub top_files: Vec<FileTouch>,
    /// 24 buckets, index = UTC hour 0..23, value = message count in that hour.
    pub time_of_day: [u64; 24],
    pub meta: ProjectMeta,
}

/// Companion totals for the section caps. Each `*_total` is the count
/// before truncation by [`ProjectLimits`].
#[derive(Debug, Clone, Serialize)]
pub struct ProjectMeta {
    pub limits: ProjectLimitsView,
    pub decisions_total: usize,
    pub todos_total: usize,
    pub threads_total: usize,
    pub files_total: usize,
    pub thread_gap_hours: i64,
    pub decisions_threshold: f32,
}

/// Plain `Serialize`-able view of [`ProjectLimits`].
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ProjectLimitsView {
    pub decisions: usize,
    pub todos: usize,
    pub threads: usize,
    pub files: usize,
}

impl From<ProjectLimits> for ProjectLimitsView {
    fn from(l: ProjectLimits) -> Self {
        Self {
            decisions: l.decisions,
            todos: l.todos,
            threads: l.threads,
            files: l.files,
        }
    }
}

/// Aggregate one project's report from already-loaded `(session, messages)`
/// pairs. The caller is expected to have filtered down to sessions matching
/// the project query — we don't re-filter on `query` here so callers can
/// reuse the same session-matching logic the rest of the CLI uses.
///
/// `query` is recorded verbatim in the output as `query`. `matched_projects`
/// is derived by deduping the input sessions' `project_name`s.
#[must_use]
pub fn aggregate(
    query: &str,
    sessions: &[(Session, Vec<Message>)],
    limits: ProjectLimits,
) -> ProjectReport {
    let session_count = sessions.len();
    let message_count: usize = sessions.iter().map(|(_, m)| m.len()).sum();

    let started_at = sessions.iter().map(|(s, _)| s.started_at).min();
    let ended_at = sessions
        .iter()
        .map(|(s, _)| s.ended_at.unwrap_or(s.started_at))
        .max();

    let token_usage = aggregate_tokens(sessions);
    let matched_projects = collect_projects(sessions);

    let (decisions_total, decisions_all) = ranked_decisions(sessions, limits.decisions);
    let (todos_total, todos_all) = ranked_todos(sessions, limits.todos);
    let (threads_total, threads_all) = clustered_threads(sessions, limits.threads);

    let (top_files_all_count, top_files) = top_files(sessions, limits.files);

    let time_of_day = time_of_day_histogram(sessions);

    ProjectReport {
        query: query.to_string(),
        matched_projects,
        session_count,
        message_count,
        started_at,
        ended_at,
        token_usage,
        decisions: decisions_all,
        todos: todos_all,
        threads: threads_all,
        top_files,
        time_of_day,
        meta: ProjectMeta {
            limits: limits.into(),
            decisions_total,
            todos_total,
            threads_total,
            files_total: top_files_all_count,
            thread_gap_hours: DEFAULT_GAP_HOURS,
            decisions_threshold: DECISIONS_THRESHOLD,
        },
    }
}

pub(crate) fn ranked_decisions(
    sessions: &[(Session, Vec<Message>)],
    limit: usize,
) -> (usize, Vec<DecisionRow>) {
    let mut rows = collect_decisions(sessions);
    rows.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
            .then_with(|| a.session_id.cmp(&b.session_id))
            .then_with(|| a.turn.cmp(&b.turn))
    });
    let total = rows.len();
    truncate_if_limited(&mut rows, limit);
    (total, rows)
}

pub(crate) fn ranked_todos(
    sessions: &[(Session, Vec<Message>)],
    limit: usize,
) -> (usize, Vec<TodoRow>) {
    let mut rows = collect_todos(sessions);
    // Newest first, mirroring `aghist todos`.
    rows.sort_by(|a, b| {
        b.timestamp
            .cmp(&a.timestamp)
            .then_with(|| a.session_id.cmp(&b.session_id))
            .then_with(|| a.turn.cmp(&b.turn))
            .then_with(|| (a.kind as u8).cmp(&(b.kind as u8)))
    });
    let total = rows.len();
    truncate_if_limited(&mut rows, limit);
    (total, rows)
}

pub(crate) fn clustered_threads(
    sessions: &[(Session, Vec<Message>)],
    limit: usize,
) -> (usize, Vec<Thread>) {
    let session_only: Vec<Session> = sessions.iter().map(|(s, _)| s.clone()).collect();
    let mut rows = threads::cluster(
        &session_only,
        ClusterOptions {
            gap: chrono::Duration::hours(DEFAULT_GAP_HOURS),
            min_sessions: 1,
        },
    );
    let total = rows.len();
    truncate_if_limited(&mut rows, limit);
    (total, rows)
}

fn truncate_if_limited<T>(rows: &mut Vec<T>, limit: usize) {
    if limit > 0 && rows.len() > limit {
        rows.truncate(limit);
    }
}

fn collect_projects(sessions: &[(Session, Vec<Message>)]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for (s, _) in sessions {
        if let Some(name) = &s.project_name {
            if !seen.iter().any(|p| p == name) {
                seen.push(name.clone());
            }
        }
    }
    seen.sort();
    seen
}

fn time_of_day_histogram(sessions: &[(Session, Vec<Message>)]) -> [u64; 24] {
    let mut hist = [0u64; 24];
    for (_, msgs) in sessions {
        for msg in msgs {
            let h = msg.timestamp.hour() as usize;
            if h < 24 {
                hist[h] = hist[h].saturating_add(1);
            }
        }
    }
    hist
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ContentBlock, Message, MessageId, Provider, Role, Session, SessionId, TokenUsage, ToolCall,
    };
    use chrono::TimeZone;
    use std::path::PathBuf;

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
        // Cost is reported as Some(0.0) — there were no unpriced sessions.
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
}
