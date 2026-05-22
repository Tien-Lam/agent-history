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
    aggregate_tokens, clustered_threads_with_refs, ranked_decisions_with_refs,
    ranked_todos_with_refs, DecisionRow, ProjectTokens, TodoRow,
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
    /// Defaults sized for a weekly review snapshot. The top-project cap keeps
    /// the Markdown report scannable; the rest mirror the per-project dashboard.
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
    aggregate_with_refs(
        window,
        sessions,
        limits,
        |_| crate::federated::LOCAL_SOURCE.to_string(),
        |session| session.session_ref().to_string(),
        |session, turn| {
            session.citation_ref(turn).map_or_else(
                || format!("{}/{}#{turn}", session.provider.slug(), session.id.0),
                |reference| reference.to_string(),
            )
        },
    )
}

#[must_use]
pub fn aggregate_with_refs(
    window: ReportWindow,
    sessions: &[(Session, Vec<Message>)],
    limits: ReportLimits,
    source_for_session: impl Fn(&Session) -> String,
    session_ref_for_session: impl Fn(&Session) -> String,
    citation_ref_for_turn: impl Fn(&Session, u32) -> String,
) -> ReportEnvelope {
    let session_count = sessions.len();
    let message_count: usize = sessions.iter().map(|(_, m)| m.len()).sum();
    let token_usage = aggregate_tokens(sessions);

    let (projects_total, top_projects) = top_projects(sessions, limits.top_projects);

    let (decisions_total, decisions_all) = ranked_decisions_with_refs(
        sessions,
        limits.decisions,
        &source_for_session,
        &citation_ref_for_turn,
    );
    let (todos_total, todos_all) = ranked_todos_with_refs(
        sessions,
        limits.todos,
        &source_for_session,
        &citation_ref_for_turn,
    );
    let (threads_total, threads_all) =
        clustered_threads_with_refs(sessions, limits.threads, &session_ref_for_session);

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
mod tests;
