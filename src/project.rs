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

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::decisions::DEFAULT_THRESHOLD as DECISIONS_THRESHOLD;
use crate::model::{Message, Session};
use crate::threads::{Thread, DEFAULT_GAP_HOURS};

mod extract;
mod files;
mod meta;
mod ranking;
mod tokens;

pub use extract::{
    collect_decisions, collect_decisions_with_refs, collect_todos, collect_todos_with_refs,
    DecisionRow, TodoRow,
};
use files::top_files;
pub use files::FileTouch;
use meta::{collect_projects, time_of_day_histogram};
pub(crate) use ranking::{
    clustered_threads_with_refs, ranked_decisions_with_refs, ranked_todos_with_refs,
};
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
    aggregate_with_refs(
        query,
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
    query: &str,
    sessions: &[(Session, Vec<Message>)],
    limits: ProjectLimits,
    source_for_session: impl Fn(&Session) -> String,
    session_ref_for_session: impl Fn(&Session) -> String,
    citation_ref_for_turn: impl Fn(&Session, u32) -> String,
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

#[cfg(test)]
mod tests;
