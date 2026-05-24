use serde::{Deserialize, Serialize};

/// What kind of indexed document a [`SearchHit`] points at. Drives the JSON
/// output shape (`kind="message"` vs `kind="note"`) and tells callers which of
/// the optional note fields are populated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HitKind {
    /// A session message - `session_id`/`message_id` identify the source turn.
    Message,
    /// A user note from the metadata sidecar - `note_id`/`note_session_ref`
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
    kind: HitKind,
    /// Internal unique session key used for cache/index lookups. The public
    /// `session_id` remains the provider-local id users already see.
    session_key: String,
    session_id: String,
    /// Internal unique message key used by pagination and embeddings.
    message_key: String,
    message_id: String,
    pub snippet: String,
    pub score: f32,
    /// `Some` when `kind == HitKind::Note`: the metadata.db row id.
    note_id: Option<i64>,
    /// `Some` when `kind == HitKind::Note`: the note's `session_ref`
    /// (`<provider>/<session-id>[#<turn>]`), as stored in the sidecar.
    note_session_ref: Option<String>,
}

impl SearchHit {
    pub fn message(
        session_key: String,
        session_id: String,
        message_key: String,
        message_id: String,
        snippet: String,
        score: f32,
    ) -> Self {
        Self {
            kind: HitKind::Message,
            session_key,
            session_id,
            message_key,
            message_id,
            snippet,
            score,
            note_id: None,
            note_session_ref: None,
        }
    }

    pub fn note(
        note_id: Option<i64>,
        note_session_ref: Option<String>,
        snippet: String,
        score: f32,
    ) -> Self {
        let message_key = note_id.map_or_else(String::new, |id| format!("note:{id}"));
        Self {
            kind: HitKind::Note,
            session_key: String::new(),
            session_id: String::new(),
            message_key,
            message_id: String::new(),
            snippet,
            score,
            note_id,
            note_session_ref,
        }
    }

    pub fn kind(&self) -> HitKind {
        self.kind
    }

    pub fn session_key(&self) -> &str {
        self.session_key.as_str()
    }

    pub fn session_id(&self) -> &str {
        self.session_id.as_str()
    }

    pub fn message_key(&self) -> &str {
        self.message_key.as_str()
    }

    pub fn message_id(&self) -> &str {
        self.message_id.as_str()
    }

    pub fn note_id(&self) -> Option<i64> {
        self.note_id
    }

    pub fn note_session_ref(&self) -> Option<&str> {
        self.note_session_ref.as_deref()
    }
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
