use std::collections::{HashMap, HashSet};
use std::ops::Bound;

use tantivy::collector::{DocSetCollector, TopDocs};
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, RangeQuery, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::{TantivyDocument, Term};

use crate::model::Role;

use super::document::{field_i64, field_text};
use super::index::SearchIndex;
use super::snippet::best_snippet;
use super::types::{HitKind, SearchError, SearchFilters, SearchHit, SemanticCandidate, RRF_K};
use super::Explanation;

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

    /// Hybrid search blending BM25 lexical ranking with caller-provided
    /// semantic candidates via Reciprocal Rank Fusion.
    ///
    /// `semantic_ranked` is the top-N from cosine similarity over the
    /// embedding store, already sorted by similarity DESC. The caller does the
    /// embedding/ranking; this method only fuses ranks. That keeps the search
    /// crate ignorant of `fastembed` so lean (no-feature) builds still link.
    ///
    /// `hybrid_weight` is the RRF weight on the semantic side, clamped to
    /// `[0.0, 1.0]`:
    /// - `0.0` -> equivalent to [`Self::search_with_filters`] (lexical only).
    /// - `1.0` -> semantic only; lexical pool only contributes if it overlaps.
    /// - `0.5` -> equal RRF blend.
    ///
    /// Both pools are filter-validated: lexical via Tantivy MUST clauses on
    /// the BM25 query; semantic via a separate Tantivy lookup that re-applies
    /// the same filters to each semantic candidate. Callers can therefore
    /// rely on `filters` having the same meaning whether hybrid is on or off.
    ///
    /// `candidate_pool` caps the per-side pool. Larger pools surface more
    /// "semantic only" hits at the cost of more Tantivy lookups; defaults to
    /// `max(limit, 50)` are reasonable.
    ///
    /// Returns `Vec<SearchHit>` whose `score` is the RRF fused score (small,
    /// roughly `[0, 2/(K+1)]`) -- not directly comparable to BM25 scores from
    /// the lexical-only path.
    pub fn search_hybrid(
        &self,
        query_str: &str,
        semantic_ranked: &[SemanticCandidate],
        limit: usize,
        filters: &SearchFilters,
        hybrid_weight: f32,
        candidate_pool: usize,
    ) -> Result<Vec<SearchHit>, SearchError> {
        let weight = hybrid_weight.clamp(0.0, 1.0);

        // Fail-open paths: weight==0 or no semantic input means "no useful
        // semantic signal", so behave exactly like the lexical-only call.
        // We intentionally do NOT short-circuit on weight==1.0; the lexical
        // pool is still useful for filter coverage when semantic misses.
        if weight == 0.0 || semantic_ranked.is_empty() {
            return self.search_with_filters(query_str, limit, filters);
        }

        let pool = candidate_pool.max(limit).max(1);

        let lexical: Vec<SearchHit> = self
            .search_inner(query_str, pool, filters, false)?
            .into_iter()
            .map(|(h, _)| h)
            .collect();

        let sem_keys: Vec<&str> = semantic_ranked
            .iter()
            .take(pool)
            .map(|c| c.message_key.as_str())
            .collect();
        let semantic: Vec<SearchHit> =
            self.fetch_filtered_by_message_keys(query_str, &sem_keys, filters)?;

        // Tuple is (lexical rank 1-based, semantic rank 1-based, hit). A None
        // rank means "absent from that pool" and contributes 0 to RRF.
        let mut by_id: HashMap<String, (Option<usize>, Option<usize>, SearchHit)> = HashMap::new();
        for (i, hit) in lexical.into_iter().enumerate() {
            by_id.insert(hit.message_key.clone(), (Some(i + 1), None, hit));
        }
        for (i, hit) in semantic.into_iter().enumerate() {
            by_id
                .entry(hit.message_key.clone())
                .and_modify(|entry| entry.1 = Some(i + 1))
                .or_insert((None, Some(i + 1), hit));
        }

        let w_sem = weight;
        let w_lex = 1.0 - weight;
        let mut fused: Vec<SearchHit> = by_id
            .into_values()
            .map(|(lex_rank, sem_rank, mut hit)| {
                #[allow(clippy::cast_precision_loss)]
                let lex_term = lex_rank.map_or(0.0, |r| w_lex / (RRF_K + r as f32));
                #[allow(clippy::cast_precision_loss)]
                let sem_term = sem_rank.map_or(0.0, |r| w_sem / (RRF_K + r as f32));
                hit.score = lex_term + sem_term;
                hit
            })
            .collect();

        // Score DESC; deterministic tie-break by (session_id ASC, message_id
        // ASC) so test snapshots and pagination cursors stay stable.
        fused.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.session_key.cmp(&b.session_key))
                .then_with(|| a.message_key.cmp(&b.message_key))
        });
        fused.truncate(limit);
        Ok(fused)
    }

    /// Look up indexed messages by internal `message_key`, applying the same filter
    /// clauses as [`Self::search_inner`]. Used by hybrid search to validate
    /// semantic candidates against server-side filters before fusing.
    ///
    /// The returned vector preserves the input order (typically similarity
    /// DESC), with non-matching ids dropped, so callers can use position as
    /// the semantic rank.
    fn fetch_filtered_by_message_keys(
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
                SearchHit {
                    kind: HitKind::Message,
                    session_key,
                    session_id,
                    message_key,
                    message_id,
                    snippet,
                    score: 0.0,
                    note_id: None,
                    note_session_ref: None,
                },
            );
        }

        // Drain in input order. Matches positionally to similarity rank.
        Ok(keys
            .iter()
            .filter_map(|key| by_msg_key.remove(*key))
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

        let combined: Box<dyn Query> = if clauses.len() == 1 {
            clauses.into_iter().next().expect("one clause").1
        } else {
            Box::new(BooleanQuery::new(clauses))
        };

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
        let combined: Box<dyn Query> = if clauses.len() == 1 {
            clauses.into_iter().next().expect("one clause").1
        } else {
            Box::new(BooleanQuery::new(clauses))
        };

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
