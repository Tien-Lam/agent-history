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
    target: SearchHitTarget,
    pub snippet: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
enum SearchHitTarget {
    Message {
        /// Internal unique session key used for cache/index lookups. The public
        /// `session_id` remains the provider-local id users already see.
        session_key: String,
        session_id: String,
        /// Internal unique message key used by pagination and embeddings.
        message_key: String,
        message_id: String,
    },
    Note {
        /// Stable note key used by pagination. Prefers metadata.db row id and
        /// falls back to the source-aware note ref for older in-memory hits.
        message_key: String,
        /// The metadata.db row id.
        note_id: Option<i64>,
        /// The note's `session_ref` (`<provider>/<session-id>[#<turn>]`), as
        /// stored in the sidecar.
        note_session_ref: Option<String>,
    },
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
            target: SearchHitTarget::Message {
                session_key,
                session_id,
                message_key,
                message_id,
            },
            snippet,
            score,
        }
    }

    pub fn note(
        note_id: Option<i64>,
        note_session_ref: Option<String>,
        snippet: String,
        score: f32,
    ) -> Self {
        let message_key = note_id.map_or_else(
            || {
                note_session_ref
                    .as_deref()
                    .map_or_else(String::new, |reference| format!("note-ref:{reference}"))
            },
            |id| format!("note:{id}"),
        );
        Self {
            target: SearchHitTarget::Note {
                message_key,
                note_id,
                note_session_ref,
            },
            snippet,
            score,
        }
    }

    pub fn kind(&self) -> HitKind {
        match self.target {
            SearchHitTarget::Message { .. } => HitKind::Message,
            SearchHitTarget::Note { .. } => HitKind::Note,
        }
    }

    pub fn session_key(&self) -> &str {
        match &self.target {
            SearchHitTarget::Message { session_key, .. } => session_key.as_str(),
            SearchHitTarget::Note { .. } => "",
        }
    }

    pub fn session_id(&self) -> &str {
        match &self.target {
            SearchHitTarget::Message { session_id, .. } => session_id.as_str(),
            SearchHitTarget::Note { .. } => "",
        }
    }

    pub fn message_key(&self) -> &str {
        match &self.target {
            SearchHitTarget::Message { message_key, .. }
            | SearchHitTarget::Note { message_key, .. } => message_key.as_str(),
        }
    }

    pub fn message_id(&self) -> &str {
        match &self.target {
            SearchHitTarget::Message { message_id, .. } => message_id.as_str(),
            SearchHitTarget::Note { .. } => "",
        }
    }

    pub fn note_id(&self) -> Option<i64> {
        match &self.target {
            SearchHitTarget::Message { .. } => None,
            SearchHitTarget::Note { note_id, .. } => *note_id,
        }
    }

    pub fn note_session_ref(&self) -> Option<&str> {
        match &self.target {
            SearchHitTarget::Message { .. } => None,
            SearchHitTarget::Note {
                note_session_ref, ..
            } => note_session_ref.as_deref(),
        }
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

#[cfg(test)]
mod tests {
    use super::SearchHit;

    #[test]
    fn note_hit_message_key_prefers_stable_note_id() {
        let hit = SearchHit::note(
            Some(42),
            Some("laptop:claude-code/session-a#1".to_string()),
            "body".to_string(),
            1.0,
        );

        assert_eq!(hit.message_key(), "note:42");
    }

    #[test]
    fn note_hit_message_key_falls_back_to_ref_when_id_missing() {
        let hit = SearchHit::note(
            None,
            Some("laptop:claude-code/session-a#1".to_string()),
            "body".to_string(),
            1.0,
        );

        assert_eq!(hit.message_key(), "note-ref:laptop:claude-code/session-a#1");
    }
}
