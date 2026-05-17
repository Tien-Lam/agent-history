use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::model::{Provider, Session};

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
pub struct ListEnvelope {
    pub sessions: Vec<SessionRow>,
    pub meta: CursorMeta,
}
