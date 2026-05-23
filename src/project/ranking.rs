use crate::model::{Message, Session};
use crate::project::{collect_decisions_with_refs, collect_todos_with_refs, DecisionRow, TodoRow};
use crate::threads::{self, ClusterOptions, Thread, DEFAULT_GAP_HOURS};

pub(crate) fn ranked_decisions_with_refs(
    sessions: &[(Session, Vec<Message>)],
    limit: usize,
    source_for_session: impl Fn(&Session) -> String,
    citation_ref_for_turn: impl Fn(&Session, u32) -> String,
) -> (usize, Vec<DecisionRow>) {
    let mut rows = collect_decisions_with_refs(sessions, source_for_session, citation_ref_for_turn);
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

pub(crate) fn ranked_todos_with_refs(
    sessions: &[(Session, Vec<Message>)],
    limit: usize,
    source_for_session: impl Fn(&Session) -> String,
    citation_ref_for_turn: impl Fn(&Session, u32) -> String,
) -> (usize, Vec<TodoRow>) {
    let mut rows = collect_todos_with_refs(sessions, source_for_session, citation_ref_for_turn);
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

pub(crate) fn clustered_threads_with_refs(
    sessions: &[(Session, Vec<Message>)],
    limit: usize,
    session_ref_for_session: impl Fn(&Session) -> String,
) -> (usize, Vec<Thread>) {
    let session_only: Vec<Session> = sessions.iter().map(|(s, _)| s.clone()).collect();
    let mut rows = threads::cluster_with_session_refs(
        &session_only,
        ClusterOptions {
            gap: chrono::Duration::hours(DEFAULT_GAP_HOURS),
            min_sessions: 1,
        },
        session_ref_for_session,
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
