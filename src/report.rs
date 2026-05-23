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

use crate::decisions::DEFAULT_THRESHOLD as DECISIONS_THRESHOLD;
use crate::model::{Message, Session};
use crate::project::{
    aggregate_tokens, clustered_threads_with_refs, ranked_decisions_with_refs,
    ranked_todos_with_refs,
};
use crate::threads::DEFAULT_GAP_HOURS;

mod projects;
mod render;
mod types;

use projects::top_projects;
pub use projects::ProjectActivity;
pub use render::render_markdown;
pub use types::{ReportEnvelope, ReportLimits, ReportLimitsView, ReportMeta, ReportWindow};

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
