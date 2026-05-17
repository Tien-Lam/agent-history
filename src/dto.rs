use std::collections::HashMap;
use std::hash::BuildHasher;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::federated::{SourceError, LOCAL_SOURCE};
use crate::model::{ContentBlock, Message, Provider, Role, Session};
use crate::search::{Explanation, HitKind, SearchHit, SearchHitCitation};

#[derive(Debug, Clone, Serialize)]
pub struct CursorMeta {
    pub next_cursor: Option<String>,
    pub total: usize,
}

impl CursorMeta {
    pub fn new(total: usize, next_cursor: Option<&str>) -> Self {
        Self {
            next_cursor: next_cursor.map(str::to_string),
            total,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionRow {
    pub id: String,
    pub source: String,
    pub provider: Provider,
    pub project: Option<String>,
    pub branch: Option<String>,
    pub summary: Option<String>,
    pub started_at: DateTime<Utc>,
    pub message_count: usize,
}

impl SessionRow {
    pub fn from_session(session: &Session, source: &str) -> Self {
        Self {
            id: session.id.0.clone(),
            source: source.to_string(),
            provider: session.provider,
            project: session.project_name.clone(),
            branch: session.git_branch.clone(),
            summary: session.summary.clone(),
            started_at: session.started_at,
            message_count: session.message_count,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct McpSessionRow {
    #[serde(flatten)]
    pub session: SessionRow,
    pub uri: String,
    pub model: Option<String>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl McpSessionRow {
    pub fn from_session(session: &Session, source: &str, uri: impl Into<String>) -> Self {
        Self {
            session: SessionRow::from_session(session, source),
            uri: uri.into(),
            model: session.model.clone(),
            ended_at: session.ended_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ListEnvelope {
    pub sessions: Vec<SessionRow>,
    pub meta: CursorMeta,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageRow {
    #[serde(rename = "ref")]
    pub ref_: Option<String>,
    pub uri: String,
    pub source: String,
    pub turn: usize,
    pub id: String,
    pub role: Role,
    pub timestamp: DateTime<Utc>,
    pub model: Option<String>,
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_target: Option<bool>,
}

impl MessageRow {
    pub fn from_message(
        message: &Message,
        source: &str,
        turn: usize,
        ref_: Option<String>,
        uri: impl Into<String>,
    ) -> Self {
        Self {
            ref_,
            uri: uri.into(),
            source: source.to_string(),
            turn,
            id: message.id.0.clone(),
            role: message.role,
            timestamp: message.timestamp,
            model: message.model.clone(),
            content: message.content.clone(),
            is_target: None,
        }
    }

    #[must_use]
    pub fn with_target(mut self, is_target: bool) -> Self {
        self.is_target = Some(is_target);
        self
    }
}

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
        match hit.kind {
            HitKind::Note => Self::from_hit(
                hit,
                None,
                source_from_note_ref(hit.note_session_ref.as_deref()),
                hit.note_session_ref.clone(),
                None,
                explanation,
            ),
            HitKind::Message => {
                let session = sessions.get(hit.session_key.as_str()).copied();
                let source = source_for_search_hit(hit, session, source_by_session);
                let citation = citations.and_then(|refs| refs.get(hit.message_key.as_str()));
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
        match hit.kind {
            HitKind::Message => Self {
                kind: HitKind::Message.slug(),
                session_id: hit.session_id.clone(),
                message_id: hit.message_id.clone(),
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
                session_id: hit.session_id.clone(),
                message_id: hit.message_id.clone(),
                score: hit.score,
                snippet: hit.snippet.clone(),
                provider: None,
                project: None,
                started_at: None,
                source: source_from_note_ref(hit.note_session_ref.as_deref()).to_string(),
                note_id: hit.note_id,
                ref_: ref_.or_else(|| hit.note_session_ref.clone()),
                turn: None,
                explanation: explanation.and_then(explanation_value),
            },
        }
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
    if let Some(source) = source_by_session.get(hit.session_key.as_str()) {
        return source;
    }
    let Some(session) = session else {
        return LOCAL_SOURCE;
    };
    source_by_session
        .get(session.identity_key().as_str())
        .map_or(LOCAL_SOURCE, String::as_str)
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

#[derive(Debug, Clone, Serialize)]
pub struct McpListResponse {
    pub total: usize,
    pub sessions: Vec<McpSessionRow>,
    pub source_errors: Vec<SourceError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpSearchResponse {
    pub query: String,
    pub limit: usize,
    pub total: usize,
    pub hits: Vec<SearchHitJson>,
    pub source_errors: Vec<SourceError>,
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

fn explanation_value(explanation: &Explanation) -> Option<Value> {
    serde_json::to_value(explanation).ok()
}
