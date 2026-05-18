use tantivy::TantivyDocument;

use crate::search::document::{field_i64, field_text};
use crate::search::index::SearchIndex;
use crate::search::snippet::best_snippet;
use crate::search::types::{HitKind, SearchHit};

impl SearchIndex {
    /// Materialize a stored Tantivy doc into a [`SearchHit`], reading the
    /// `kind` field to drive whether note metadata is populated. Centralized
    /// so message-only and hybrid paths both produce a uniformly-shaped hit.
    pub(crate) fn doc_to_hit(
        &self,
        doc: &TantivyDocument,
        query_str: &str,
        score: f32,
    ) -> SearchHit {
        let kind = if field_text(doc, self.fields.kind) == HitKind::Note.slug() {
            HitKind::Note
        } else {
            HitKind::Message
        };
        let session_key = field_text(doc, self.fields.session_key);
        let session_id = field_text(doc, self.fields.session_id);
        let mut message_key = field_text(doc, self.fields.message_key);
        let message_id = field_text(doc, self.fields.message_id);
        let content = field_text(doc, self.fields.content);
        let tool_output = field_text(doc, self.fields.tool_output);
        let snippet = best_snippet(&content, &tool_output, query_str, 120);
        let (note_id, note_session_ref) = match kind {
            HitKind::Note => {
                let r = field_text(doc, self.fields.note_session_ref);
                let r = if r.is_empty() { None } else { Some(r) };
                let id = field_i64(doc, self.fields.note_id);
                if let Some(id) = id {
                    message_key = format!("note:{id}");
                }
                (id, r)
            }
            HitKind::Message => (None, None),
        };
        SearchHit {
            kind,
            session_key,
            session_id,
            message_key,
            message_id,
            snippet,
            score,
            note_id,
            note_session_ref,
        }
    }
}
