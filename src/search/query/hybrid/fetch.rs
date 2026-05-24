use std::collections::HashMap;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::{TantivyDocument, Term};

use crate::search::document::field_text;
use crate::search::index::SearchIndex;
use crate::search::snippet::best_snippet;
use crate::search::types::{SearchError, SearchFilters, SearchHit};

impl SearchIndex {
    /// Look up indexed messages by internal `message_key`, applying the same
    /// filter clauses as [`Self::search_inner`]. Used by hybrid search to
    /// validate semantic candidates against server-side filters before fusing.
    ///
    /// The returned vector preserves the input order (typically similarity
    /// DESC), with non-matching ids dropped, so callers can use position as
    /// the semantic rank.
    pub(super) fn fetch_filtered_by_message_keys(
        &self,
        query_str: &str,
        keys: &[&str],
        filters: &SearchFilters,
    ) -> Result<Vec<SearchHit>, SearchError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }

        self.reader.reload()?;
        let searcher = self.reader.searcher();

        // OR of message_key terms. Tantivy doesn't have an IN query, so we
        // build a Should-clause boolean.
        let id_clauses: Vec<(Occur, Box<dyn Query>)> = keys
            .iter()
            .map(|key| {
                let term = Term::from_field_text(self.fields.message_key, key);
                (
                    Occur::Should,
                    Box::new(TermQuery::new(term, IndexRecordOption::Basic)) as Box<dyn Query>,
                )
            })
            .collect();
        let id_query: Box<dyn Query> = Box::new(BooleanQuery::new(id_clauses));

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::with_capacity(5);
        clauses.push((Occur::Must, id_query));
        self.add_filter_clauses(&mut clauses, filters);

        let combined: Box<dyn Query> = Box::new(BooleanQuery::new(clauses));

        // Cap at the input length; we have at most one hit per requested key.
        let top_docs =
            searcher.search(&combined, &TopDocs::with_limit(keys.len()).order_by_score())?;

        let project_needle = Self::project_filter_needle(filters);

        let mut by_msg_key: HashMap<String, SearchHit> = HashMap::new();
        for (_score, addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(addr)?;
            if !self.matches_project_filter(&doc, project_needle.as_deref()) {
                continue;
            }
            let session_key = field_text(&doc, self.fields.session_key);
            let session_id = field_text(&doc, self.fields.session_id);
            let message_key = field_text(&doc, self.fields.message_key);
            let message_id = field_text(&doc, self.fields.message_id);
            let content = field_text(&doc, self.fields.content);
            let tool_output = field_text(&doc, self.fields.tool_output);
            let snippet = best_snippet(&content, &tool_output, query_str, 120);
            by_msg_key.insert(
                message_key.clone(),
                SearchHit::message(
                    session_key,
                    session_id,
                    message_key,
                    message_id,
                    snippet,
                    0.0,
                ),
            );
        }

        // Drain in input order. Matches positionally to similarity rank.
        Ok(keys
            .iter()
            .filter_map(|key| by_msg_key.remove(*key))
            .collect())
    }
}
