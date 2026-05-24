use tantivy::collector::{Count, TopDocs};
use tantivy::query::{Occur, Query, QueryParser};
use tantivy::{Searcher, TantivyDocument};

use super::combined_query;
use crate::search::index::SearchIndex;
use crate::search::types::{SearchError, SearchFilters, SearchHit};
use crate::search::Explanation;

const PROJECT_FILTER_PAGE_SIZE: usize = 128;

pub(crate) struct SearchInnerOutput {
    pub(crate) hits: Vec<(SearchHit, Option<Explanation>)>,
    pub(crate) total: usize,
}

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
        Ok(self
            .search_inner_with_total(query_str, limit, filters, explain)?
            .hits)
    }

    pub(crate) fn search_inner_with_total(
        &self,
        query_str: &str,
        limit: usize,
        filters: &SearchFilters,
        explain: bool,
    ) -> Result<SearchInnerOutput, SearchError> {
        if query_str.trim().is_empty() {
            return Ok(SearchInnerOutput {
                hits: Vec::new(),
                total: 0,
            });
        }
        if limit == 0 {
            return Ok(SearchInnerOutput {
                hits: Vec::new(),
                total: 0,
            });
        }

        self.reader.reload()?;
        let searcher = self.reader.searcher();
        let combined = self.combined_search_query(query_str, filters)?;

        let project_needle = Self::project_filter_needle(filters);
        if let Some(project_needle) = project_needle.as_deref() {
            return self.search_project_filtered(
                &searcher,
                &*combined,
                query_str,
                limit,
                project_needle,
                explain,
            );
        }

        let (top_docs, total) = searcher.search(
            &combined,
            &(TopDocs::with_limit(limit).order_by_score(), Count),
        )?;

        let mut hits = Vec::with_capacity(top_docs.len().min(limit));
        for (score, addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(addr)?;
            let hit = self.doc_to_hit(&doc, query_str, score);
            let explanation = if explain {
                Some(combined.explain(&searcher, addr)?)
            } else {
                None
            };
            hits.push((hit, explanation));
        }

        Ok(SearchInnerOutput { hits, total })
    }

    fn search_project_filtered(
        &self,
        searcher: &Searcher,
        query: &dyn Query,
        query_str: &str,
        limit: usize,
        project_needle: &str,
        explain: bool,
    ) -> Result<SearchInnerOutput, SearchError> {
        let pre_filter_total = searcher.search(query, &Count)?;
        let page_size = PROJECT_FILTER_PAGE_SIZE.max(limit);
        let mut offset = 0usize;
        let mut total = 0usize;
        let mut hits = Vec::with_capacity(limit);

        while offset < pre_filter_total {
            let top_docs = searcher.search(
                query,
                &TopDocs::with_limit(page_size)
                    .and_offset(offset)
                    .order_by_score(),
            )?;
            if top_docs.is_empty() {
                break;
            }

            for (score, addr) in top_docs {
                let doc: TantivyDocument = searcher.doc(addr)?;
                if !self.matches_project_filter(&doc, Some(project_needle)) {
                    continue;
                }
                total = total.saturating_add(1);
                if hits.len() >= limit {
                    continue;
                }

                let hit = self.doc_to_hit(&doc, query_str, score);
                let explanation = if explain {
                    Some(query.explain(searcher, addr)?)
                } else {
                    None
                };
                hits.push((hit, explanation));
            }

            offset = offset.saturating_add(page_size);
        }

        Ok(SearchInnerOutput { hits, total })
    }

    fn combined_search_query(
        &self,
        query_str: &str,
        filters: &SearchFilters,
    ) -> Result<Box<dyn Query>, SearchError> {
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
        Ok(combined)
    }
}
