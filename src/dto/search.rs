use std::collections::HashMap;
use std::hash::BuildHasher;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::federated::LOCAL_SOURCE;
use crate::model::{Provider, Session};
use crate::search::{Explanation, HitKind, SearchHit, SearchHitCitation};

#[derive(Debug, Clone, Serialize)]
pub struct SearchHitJson {
    pub kind: &'static str,
    pub session_id: String,
    pub message_id: String,
    pub score: f32,
    pub snippet: String,
    pub provider: Option<Provider>,
    pub project: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_id: Option<i64>,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<Value>,
}

impl SearchHitJson {
    pub fn from_search_hit<SessionHasher, SourceHasher>(
        hit: &SearchHit,
        explanation: Option<&Explanation>,
        sessions: &HashMap<String, &Session, SessionHasher>,
        source_by_session: &HashMap<String, String, SourceHasher>,
        citations: Option<&HashMap<String, SearchHitCitation>>,
    ) -> Self
    where
        SessionHasher: BuildHasher,
        SourceHasher: BuildHasher,
    {
        match hit.kind() {
            HitKind::Note => Self::from_hit(
                hit,
                None,
                source_from_note_ref(hit.note_session_ref()),
                hit.note_session_ref().map(str::to_string),
                None,
                explanation,
            ),
            HitKind::Message => {
                let session = sessions.get(hit.session_key()).copied();
                let source = source_for_search_hit(hit, session, source_by_session);
                let citation = citations.and_then(|refs| refs.get(hit.message_key()));
                Self::from_hit(
                    hit,
                    session,
                    source,
                    citation.map(|citation| citation.ref_.clone()),
                    citation.map(|citation| citation.turn),
                    explanation,
                )
            }
        }
    }

    pub fn from_hit(
        hit: &SearchHit,
        session: Option<&Session>,
        source: &str,
        ref_: Option<String>,
        turn: Option<usize>,
        explanation: Option<&Explanation>,
    ) -> Self {
        match hit.kind() {
            HitKind::Message => Self {
                kind: HitKind::Message.slug(),
                session_id: hit.session_id().to_string(),
                message_id: hit.message_id().to_string(),
                score: hit.score,
                snippet: hit.snippet.clone(),
                provider: session.map(|s| s.provider),
                project: session.and_then(|s| s.project_name.clone()),
                started_at: session.map(|s| s.started_at),
                source: source.to_string(),
                note_id: None,
                ref_,
                turn,
                explanation: explanation.and_then(explanation_value),
            },
            HitKind::Note => Self {
                kind: HitKind::Note.slug(),
                session_id: hit.session_id().to_string(),
                message_id: hit.message_id().to_string(),
                score: hit.score,
                snippet: hit.snippet.clone(),
                provider: None,
                project: None,
                started_at: None,
                source: source_from_note_ref(hit.note_session_ref()).to_string(),
                note_id: hit.note_id(),
                ref_: ref_.or_else(|| hit.note_session_ref().map(str::to_string)),
                turn: None,
                explanation: explanation.and_then(explanation_value),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchMeta {
    pub next_cursor: Option<String>,
    pub total: usize,
    pub engine: String,
}

impl SearchMeta {
    pub fn new(total: usize, next_cursor: Option<&str>, engine: &str) -> Self {
        Self {
            next_cursor: next_cursor.map(str::to_string),
            total,
            engine: engine.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchEnvelope {
    pub hits: Vec<SearchHitJson>,
    pub meta: SearchMeta,
}

pub fn source_from_note_ref(reference: Option<&str>) -> &str {
    let Some(reference) = reference else {
        return LOCAL_SOURCE;
    };
    let slash = reference.find('/');
    let colon = reference.find(':');
    match (colon, slash) {
        (Some(c), Some(s)) if c < s => &reference[..c],
        _ => LOCAL_SOURCE,
    }
}

fn source_for_search_hit<'a, SourceHasher>(
    hit: &SearchHit,
    session: Option<&Session>,
    source_by_session: &'a HashMap<String, String, SourceHasher>,
) -> &'a str
where
    SourceHasher: BuildHasher,
{
    if let Some(source) = source_by_session.get(hit.session_key()) {
        return source;
    }
    let Some(session) = session else {
        return LOCAL_SOURCE;
    };
    source_by_session
        .get(session.identity_key().as_str())
        .map_or(LOCAL_SOURCE, String::as_str)
}

fn explanation_value(explanation: &Explanation) -> Option<Value> {
    serde_json::to_value(explanation).ok()
}
