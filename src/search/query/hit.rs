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
        let content = field_text(doc, self.fields.content);
        let tool_output = field_text(doc, self.fields.tool_output);
        let snippet = best_snippet(&content, &tool_output, query_str, 120);

        if field_text(doc, self.fields.kind) == HitKind::Note.slug() {
            let note_session_ref = field_text(doc, self.fields.note_session_ref);
            let note_session_ref = if note_session_ref.is_empty() {
                None
            } else {
                Some(note_session_ref)
            };
            return SearchHit::note(
                field_i64(doc, self.fields.note_id),
                note_session_ref,
                snippet,
                score,
            );
        }

        SearchHit::message(
            field_text(doc, self.fields.session_key),
            field_text(doc, self.fields.session_id),
            field_text(doc, self.fields.message_key),
            field_text(doc, self.fields.message_id),
            snippet,
            score,
        )
    }
}
