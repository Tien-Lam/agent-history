use std::collections::HashMap;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::{TantivyDocument, Term};

use crate::search::document::field_text;
use crate::search::index::SearchIndex;
use crate::search::snippet::best_snippet;
use crate::search::types::{
    HitKind, SearchError, SearchFilters, SearchHit, SemanticCandidate, RRF_K,
};

impl SearchIndex {
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
}
