use std::collections::HashSet;

use tantivy::collector::DocSetCollector;
use tantivy::query::{Occur, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::{TantivyDocument, Term};

use super::combined_query;
use crate::model::Role;
use crate::search::document::field_text;
use crate::search::index::SearchIndex;
use crate::search::types::SearchError;

impl SearchIndex {
    /// Return the set of session IDs that contain at least one indexed message
    /// matching the given message-level filters. Used by the TUI filter panel
    /// to live-filter the session list by role / has-tool-call without
    /// loading every session's messages into memory.
    ///
    /// Returns an empty set when no filter is active (caller should treat
    /// "no filter" as "no constraint", not "show nothing").
    pub fn session_ids_with_messages(
        &self,
        role: Option<Role>,
        has_tool_call: bool,
    ) -> Result<HashSet<String>, SearchError> {
        if role.is_none() && !has_tool_call {
            return Ok(HashSet::new());
        }

        self.reader.reload()?;
        let searcher = self.reader.searcher();

        let mut clauses = Vec::new();
        if let Some(role) = role {
            let term = Term::from_field_text(self.fields.role, role.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)) as _,
            ));
        }
        if has_tool_call {
            let term = Term::from_field_i64(self.fields.has_tool_call, 1);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)) as _,
            ));
        }
        let combined = combined_query(clauses);

        // Walk every matching message and collect distinct internal session keys.
        // Ranking is irrelevant here; `DocSetCollector` is cheaper than
        // `TopDocs` because it skips score tracking and has no top-N cap.
        let docs = searcher.search(&combined, &DocSetCollector)?;

        let mut session_ids = HashSet::new();
        for addr in docs {
            let doc: TantivyDocument = searcher.doc(addr)?;
            session_ids.insert(field_text(&doc, self.fields.session_key));
        }
        Ok(session_ids)
    }
}
