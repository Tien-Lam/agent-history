use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::federated::SourceError;
use crate::model::Session;

use super::list::SessionRow;
use super::search::SearchHitJson;

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
