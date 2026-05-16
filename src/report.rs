//! Cross-project narrative summary. Powers `aghist report`.
//!
//! Aggregates a window of activity (default: last 7 days) across every
//! provider into a single envelope: top active projects, decision count,
//! open TODOs, completed work threads, and headline token totals. The
//! per-section heuristics reuse [`crate::project`] so the shapes match
//! the per-project dashboard.
//!
//! There is no LLM here — agents that want richer extraction can post-
//! process the citation refs via `aghist show`.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::decisions::DEFAULT_THRESHOLD as DECISIONS_THRESHOLD;
use crate::model::{Message, Session};
use crate::project::{
    aggregate_tokens, clustered_threads, ranked_decisions, ranked_todos, DecisionRow,
    ProjectTokens, TodoRow,
};
use crate::threads::{Thread, DEFAULT_GAP_HOURS};

mod projects;
mod render;

use projects::top_projects;
pub use projects::ProjectActivity;
pub use render::render_markdown;

/// Per-section caps. Zero means "no cap" for the corresponding section.
#[derive(Debug, Clone, Copy)]
pub struct ReportLimits {
    pub top_projects: usize,
    pub decisions: usize,
    pub todos: usize,
    pub threads: usize,
}

impl ReportLimits {
    /// Defaults sized for a weekly review snapshot. The bead spec calls for
    /// "top 3 active projects"; the rest mirror the per-project dashboard.
    pub const DEFAULTS: Self = Self {
        top_projects: 3,
        decisions: 5,
        todos: 10,
        threads: 5,
    };
}

impl Default for ReportLimits {
    fn default() -> Self {
        Self::DEFAULTS
    }
}

/// Time window covered by the report. `days` is precomputed from
/// `(ended_at - started_at)` rounded up to the nearest whole day so it is
/// stable and self-describing in the JSON envelope.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ReportWindow {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub days: i64,
}

impl ReportWindow {
    /// Build a window ending at `end` and reaching `days` days back. `days`
    /// is clamped to at least 1 so the envelope's `days` field is always
    /// positive even for zero-/negative-input edge cases.
    #[must_use]
    pub fn last_days(end: DateTime<Utc>, days: i64) -> Self {
        let days = days.max(1);
        Self {
            started_at: end - Duration::days(days),
            ended_at: end,
            days,
        }
    }

    /// Build a window from explicit bounds. `days` is computed as the
    /// span in whole days, rounded up; minimum 1.
    #[must_use]
    pub fn between(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        let span = end.signed_duration_since(start);
        // Round up so a 6h12m window is reported as "1 day", and a 7-day
        // window is "7 days" rather than "6" due to truncation.
        let secs = span.num_seconds().max(0);
        let one_day = Duration::days(1).num_seconds();
        let days = (secs + one_day - 1) / one_day;
        Self {
            started_at: start,
            ended_at: end,
            days: days.max(1),
        }
    }
}

/// The cross-project weekly-summary envelope.
#[derive(Debug, Clone, Serialize)]
pub struct ReportEnvelope {
    pub window: ReportWindow,
    pub session_count: usize,
    pub message_count: usize,
    pub project_count: usize,
    pub token_usage: ProjectTokens,
    pub top_projects: Vec<ProjectActivity>,
    pub decisions: Vec<DecisionRow>,
    pub todos: Vec<TodoRow>,
    pub threads: Vec<Thread>,
    pub meta: ReportMeta,
}

/// Companion totals for the section caps. Each `*_total` is the count
/// before truncation by [`ReportLimits`].
#[derive(Debug, Clone, Serialize)]
pub struct ReportMeta {
    pub limits: ReportLimitsView,
    pub projects_total: usize,
    pub decisions_total: usize,
    pub todos_total: usize,
    pub threads_total: usize,
    pub thread_gap_hours: i64,
    pub decisions_threshold: f32,
}

/// Plain `Serialize`-able view of [`ReportLimits`].
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ReportLimitsView {
    pub top_projects: usize,
    pub decisions: usize,
    pub todos: usize,
    pub threads: usize,
}

impl From<ReportLimits> for ReportLimitsView {
    fn from(l: ReportLimits) -> Self {
        Self {
            top_projects: l.top_projects,
            decisions: l.decisions,
            todos: l.todos,
            threads: l.threads,
        }
    }
}

/// Aggregate a report from already-window-filtered `(session, messages)`
/// pairs. The caller is expected to have filtered down to sessions inside
/// `window` — we don't re-filter here so callers can reuse the same
/// session-matching logic the rest of the CLI uses.
#[must_use]
pub fn aggregate(
    window: ReportWindow,
    sessions: &[(Session, Vec<Message>)],
    limits: ReportLimits,
) -> ReportEnvelope {
    let session_count = sessions.len();
    let message_count: usize = sessions.iter().map(|(_, m)| m.len()).sum();
    let token_usage = aggregate_tokens(sessions);

    let (projects_total, top_projects) = top_projects(sessions, limits.top_projects);

    let (decisions_total, decisions_all) = ranked_decisions(sessions, limits.decisions);
    let (todos_total, todos_all) = ranked_todos(sessions, limits.todos);
    let (threads_total, threads_all) = clustered_threads(sessions, limits.threads);

    ReportEnvelope {
        window,
        session_count,
        message_count,
        project_count: projects_total,
        token_usage,
        top_projects,
        decisions: decisions_all,
        todos: todos_all,
        threads: threads_all,
        meta: ReportMeta {
            limits: limits.into(),
            projects_total,
            decisions_total,
            todos_total,
            threads_total,
            thread_gap_hours: DEFAULT_GAP_HOURS,
            decisions_threshold: DECISIONS_THRESHOLD,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentBlock, MessageId, Provider, Role, SessionId, TokenUsage};
    use chrono::TimeZone;
    use std::path::PathBuf;

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
        // alpha has more messages than beta → ranks first.
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
        // delta has the most messages (4), gamma next (3).
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
}
