use std::path::PathBuf;

use crate::model::{Provider, Role};
use chrono::{DateTime, Utc};

mod manifest;

pub(super) use manifest::{FileFingerprint, Manifest};

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("index error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("query parse error: {0}")]
    QueryParse(#[from] tantivy::query::QueryParserError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "refusing to reset index directory {path}: it contains non-aghist entries ({entries})"
    )]
    UnsafeIndexDir { path: PathBuf, entries: String },
}

/// What kind of indexed document a [`SearchHit`] points at. Drives the JSON
/// output shape (`kind="message"` vs `kind="note"`) and tells callers which of
/// the optional note fields are populated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    /// A session message — `session_id`/`message_id` identify the source turn.
    Message,
    /// A user note from the metadata sidecar — `note_id`/`note_session_ref`
    /// identify the row; `session_id`/`message_id` are empty.
    Note,
}

impl HitKind {
    /// Stable lowercase slug used in JSON output and the indexed `kind` field.
    pub fn slug(self) -> &'static str {
        match self {
            HitKind::Message => "message",
            HitKind::Note => "note",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub kind: HitKind,
    /// Internal unique session key used for cache/index lookups. The public
    /// `session_id` remains the provider-local id users already see.
    pub session_key: String,
    pub session_id: String,
    /// Internal unique message key used by pagination and embeddings.
    pub message_key: String,
    pub message_id: String,
    pub snippet: String,
    pub score: f32,
    /// `Some` when `kind == HitKind::Note`: the metadata.db row id.
    pub note_id: Option<i64>,
    /// `Some` when `kind == HitKind::Note`: the note's `session_ref`
    /// (`<provider>/<session-id>[#<turn>]`), as stored in the sidecar.
    pub note_session_ref: Option<String>,
}

/// One semantic candidate sourced from cosine similarity over the embedding
/// store. Callers rank the full store and pass the top-N here; search storage
/// stays ignorant of the embedding pipeline.
#[derive(Debug, Clone)]
pub struct SemanticCandidate {
    pub message_key: String,
    pub message_id: String,
    pub similarity: f32,
}

/// Reciprocal Rank Fusion smoothing constant. 60 is the value from Cormack
/// et al.'s original paper and the one most production hybrid-search systems
/// use; large enough to dampen the penalty for rank-1 vs rank-2 differences,
/// small enough that rank still matters.
pub const RRF_K: f32 = 60.0;

#[derive(Debug, Default, Clone)]
pub struct IndexStats {
    /// Sessions written this pass (added + updated).
    pub sessions_indexed: usize,
    /// Messages written this pass.
    pub messages_indexed: usize,
    /// Sessions never seen by the manifest before.
    pub added: usize,
    /// Sessions that existed in the manifest but had a newer source mtime.
    pub updated: usize,
    /// Sessions that the manifest already had at the current mtime — skipped.
    pub unchanged: usize,
    /// Sessions present in the manifest but no longer discovered this pass.
    pub removed: usize,
    /// Discovered sessions that could not be loaded and therefore were not indexed.
    pub load_errors: Vec<IndexLoadError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexLoadError {
    pub provider: Provider,
    pub session_id: String,
    pub error: String,
}

/// Outcome of a single index-notes pass. Mirrors [`IndexStats`] in spirit but
/// counts metadata.db note rows instead of session files.
#[derive(Debug, Default, Clone)]
pub struct NotesIndexStats {
    /// Notes never seen by the manifest before.
    pub added: usize,
    /// Notes whose `updated_at` advanced since the manifest snapshot.
    pub updated: usize,
    /// Notes already present at the current `updated_at` — skipped.
    pub unchanged: usize,
    /// Notes present in the manifest but absent from the input list — pruned
    /// from the index so deletes in the sidecar propagate to search.
    pub removed: usize,
}

/// Server-side filters applied alongside a `search` query. Empty fields mean
/// "do not filter on this dimension". `since`/`until` are inclusive bounds on
/// the message timestamp; `project` is a case-insensitive substring match.
#[derive(Debug, Default, Clone)]
pub struct SearchFilters {
    pub provider: Option<Provider>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub project: Option<String>,
    pub role: Option<Role>,
    pub has_tool_call: bool,
}

impl SearchFilters {
    pub fn is_empty(&self) -> bool {
        self.provider.is_none()
            && self.since.is_none()
            && self.until.is_none()
            && self.project.is_none()
            && self.role.is_none()
            && !self.has_tool_call
    }
}
