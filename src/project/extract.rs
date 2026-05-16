use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::decisions::{self, DEFAULT_THRESHOLD as DECISIONS_THRESHOLD};
use crate::federated;
use crate::model::{Message, Provider, Session};
use crate::todos::{self, TodoCandidate, TodoKind};

/// One ranked decision-candidate with its citation ref expanded.
#[derive(Debug, Clone, Serialize)]
pub struct DecisionRow {
    #[serde(rename = "ref")]
    pub reference: String,
    pub source: String,
    pub provider: Provider,
    pub session_id: String,
    pub turn: u32,
    pub score: f32,
    pub markers: Vec<String>,
    pub snippet: String,
    pub timestamp: DateTime<Utc>,
}

/// One TODO candidate with its citation ref expanded.
#[derive(Debug, Clone, Serialize)]
pub struct TodoRow {
    #[serde(rename = "ref")]
    pub reference: String,
    pub source: String,
    pub provider: Provider,
    pub session_id: String,
    pub turn: u32,
    pub kind: TodoKind,
    pub snippet: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bd_id: Option<String>,
}

/// Collect decision-candidate rows across all messages in `sessions`,
/// filtered by [`crate::decisions::DEFAULT_THRESHOLD`]. The returned rows
/// are unsorted; callers typically sort by `score` desc.
#[must_use]
pub fn collect_decisions(sessions: &[(Session, Vec<Message>)]) -> Vec<DecisionRow> {
    collect_decisions_with_refs(
        sessions,
        |_| federated::LOCAL_SOURCE.to_string(),
        default_citation_ref,
    )
}

#[must_use]
pub fn collect_decisions_with_refs(
    sessions: &[(Session, Vec<Message>)],
    source_for_session: impl Fn(&Session) -> String,
    citation_ref_for_turn: impl Fn(&Session, u32) -> String,
) -> Vec<DecisionRow> {
    let mut out = Vec::new();
    for (s, msgs) in sessions {
        let source = source_for_session(s);
        for (idx, msg) in msgs.iter().enumerate() {
            let turn = u32::try_from(idx + 1).unwrap_or(u32::MAX);
            for c in decisions::extract_from_message(msg, turn, DECISIONS_THRESHOLD) {
                out.push(DecisionRow {
                    reference: citation_ref_for_turn(s, c.turn),
                    source: source.clone(),
                    provider: s.provider,
                    session_id: s.id.0.clone(),
                    turn: c.turn,
                    score: c.score,
                    markers: c.markers,
                    snippet: c.snippet,
                    timestamp: c.timestamp,
                });
            }
        }
    }
    out
}

/// Collect TODO-candidate rows across all messages in `sessions`. The
/// returned rows are unsorted; callers typically sort newest-first.
#[must_use]
pub fn collect_todos(sessions: &[(Session, Vec<Message>)]) -> Vec<TodoRow> {
    collect_todos_with_refs(
        sessions,
        |_| federated::LOCAL_SOURCE.to_string(),
        default_citation_ref,
    )
}

#[must_use]
pub fn collect_todos_with_refs(
    sessions: &[(Session, Vec<Message>)],
    source_for_session: impl Fn(&Session) -> String,
    citation_ref_for_turn: impl Fn(&Session, u32) -> String,
) -> Vec<TodoRow> {
    let mut out = Vec::new();
    for (s, msgs) in sessions {
        let source = source_for_session(s);
        let candidates: Vec<TodoCandidate> =
            todos::extract_from_messages(s.provider, &s.id, msgs, &[]);
        for c in candidates {
            out.push(TodoRow {
                reference: citation_ref_for_turn(s, c.citation.turn),
                source: source.clone(),
                provider: c.citation.provider,
                session_id: c.citation.session_id.0.clone(),
                turn: c.citation.turn,
                kind: c.kind,
                snippet: c.snippet,
                timestamp: c.timestamp,
                bd_id: c.bd_id,
            });
        }
    }
    out
}

fn default_citation_ref(session: &Session, turn: u32) -> String {
    session.citation_ref(turn).map_or_else(
        || format!("{}/{}#{turn}", session.provider.slug(), session.id.0),
        |reference| reference.to_string(),
    )
}
