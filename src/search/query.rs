use std::collections::HashSet;
use std::ops::Bound;

use tantivy::collector::{DocSetCollector, TopDocs};
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, RangeQuery, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::{TantivyDocument, Term};

use crate::model::Role;

use super::document::{field_i64, field_text};
use super::index::SearchIndex;
use super::snippet::best_snippet;
use super::types::{HitKind, SearchError, SearchFilters, SearchHit};
use super::Explanation;

mod hybrid;

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

    fn add_filter_clauses(
        &self,
        clauses: &mut Vec<(Occur, Box<dyn Query>)>,
        filters: &SearchFilters,
    ) {
        if let Some(provider) = filters.provider {
            let term = Term::from_field_text(self.fields.provider, provider.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if let Some(role) = filters.role {
            let term = Term::from_field_text(self.fields.role, role.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if filters.has_tool_call {
            let term = Term::from_field_i64(self.fields.has_tool_call, 1);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if filters.since.is_some() || filters.until.is_some() {
            let lower = filters.since.map_or(Bound::Unbounded, |t| {
                Bound::Included(Term::from_field_i64(self.fields.timestamp, t.timestamp()))
            });
            let upper = filters.until.map_or(Bound::Unbounded, |t| {
                Bound::Included(Term::from_field_i64(self.fields.timestamp, t.timestamp()))
            });
            clauses.push((Occur::Must, Box::new(RangeQuery::new(lower, upper))));
        }
    }

    fn project_filter_needle(filters: &SearchFilters) -> Option<String> {
        filters
            .project
            .as_deref()
            .map(str::to_lowercase)
            .filter(|s| !s.is_empty())
    }

    fn matches_project_filter(&self, doc: &TantivyDocument, needle: Option<&str>) -> bool {
        let Some(needle) = needle else {
            return true;
        };
        field_text(doc, self.fields.project_raw)
            .to_lowercase()
            .contains(needle)
    }

    fn search_inner(
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

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        if let Some(role) = role {
            let term = Term::from_field_text(self.fields.role, role.slug());
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }
        if has_tool_call {
            let term = Term::from_field_i64(self.fields.has_tool_call, 1);
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
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

    /// Materialize a stored Tantivy doc into a [`SearchHit`], reading the
    /// `kind` field to drive whether note metadata is populated. Centralized
    /// so message-only and hybrid paths both produce a uniformly-shaped hit.
    fn doc_to_hit(&self, doc: &TantivyDocument, query_str: &str, score: f32) -> SearchHit {
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

fn combined_query(mut clauses: Vec<(Occur, Box<dyn Query>)>) -> Box<dyn Query> {
    if clauses.len() == 1 {
        clauses.remove(0).1
    } else {
        Box::new(BooleanQuery::new(clauses))
    }
}
