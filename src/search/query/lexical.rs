use tantivy::collector::TopDocs;
use tantivy::query::{Occur, Query, QueryParser};
use tantivy::TantivyDocument;

use super::combined_query;
use crate::search::index::SearchIndex;
use crate::search::types::{SearchError, SearchFilters, SearchHit};
use crate::search::Explanation;

impl SearchIndex {
    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchHit>, SearchError> {
        self.search_with_filters(query_str, limit, &SearchFilters::default())
    }

    /// Search with structured filters applied alongside the user query.
    ///
    /// Provider/role/timestamp/has-tool-call filters are pushed into Tantivy as
    /// boolean MUST clauses (cheap, scaled by the index). The project filter is
    /// applied as a post-filter substring match against the stored project
    /// value, since project names can contain arbitrary characters that don't
    /// round-trip cleanly through the analyzed `project` text field.
    pub fn search_with_filters(
        &self,
        query_str: &str,
        limit: usize,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchHit>, SearchError> {
        Ok(self
            .search_inner(query_str, limit, filters, false)?
            .into_iter()
            .map(|(hit, _)| hit)
            .collect())
    }

    /// Like [`Self::search_with_filters`] but also returns Tantivy's BM25
    /// [`Explanation`] tree for each hit, so callers can surface a score
    /// breakdown (the `--debug-search` flag).
    pub fn search_with_filters_and_explain(
        &self,
        query_str: &str,
        limit: usize,
        filters: &SearchFilters,
    ) -> Result<Vec<(SearchHit, Explanation)>, SearchError> {
        let raw = self.search_inner(query_str, limit, filters, true)?;
        Ok(raw
            .into_iter()
            .map(|(hit, explain)| {
                // explain=true guarantees Some; fall back to a stub if Tantivy
                // ever returns a hit without an explainable scorer.
                let explanation = explain.unwrap_or_else(|| {
                    Explanation::new_with_string("no explanation available".into(), hit.score)
                });
                (hit, explanation)
            })
            .collect())
    }

    pub(crate) fn search_inner(
        &self,
        query_str: &str,
        limit: usize,
        filters: &SearchFilters,
        explain: bool,
    ) -> Result<Vec<(SearchHit, Option<Explanation>)>, SearchError> {
        if query_str.trim().is_empty() {
            return Ok(Vec::new());
        }

        self.reader.reload()?;
        let searcher = self.reader.searcher();

        let parser = QueryParser::for_index(
            &self.index,
            vec![
                self.fields.content,
                self.fields.project,
                self.fields.tool_output,
            ],
        );
        let user_query = parser.parse_query(query_str)?;

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::with_capacity(6);
        clauses.push((Occur::Must, user_query));
        self.add_filter_clauses(&mut clauses, filters);

        let combined = combined_query(clauses);

        // Project is post-filtered; over-fetch to keep results stable when a
        // restrictive project filter would otherwise prune the limit-N window.
        let project_needle = Self::project_filter_needle(filters);
        let fetch_limit = if project_needle.is_some() {
            limit.saturating_mul(8).max(limit)
        } else {
            limit
        };

        let top_docs = searcher.search(
            &combined,
            &TopDocs::with_limit(fetch_limit).order_by_score(),
        )?;

        let mut hits = Vec::with_capacity(top_docs.len().min(limit));
        for (score, addr) in top_docs {
            if hits.len() >= limit {
                break;
            }
            let doc: TantivyDocument = searcher.doc(addr)?;
            if !self.matches_project_filter(&doc, project_needle.as_deref()) {
                continue;
            }
            let hit = self.doc_to_hit(&doc, query_str, score);
            let explanation = if explain {
                Some(combined.explain(&searcher, addr)?)
            } else {
                None
            };
            hits.push((hit, explanation));
        }

        Ok(hits)
    }
}
