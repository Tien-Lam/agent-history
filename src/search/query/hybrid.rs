use std::collections::HashMap;

use crate::search::index::SearchIndex;
use crate::search::types::{SearchError, SearchFilters, SearchHit, SemanticCandidate, RRF_K};

mod fetch;

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
            by_id.insert(hit.message_key().to_string(), (Some(i + 1), None, hit));
        }
        for (i, hit) in semantic.into_iter().enumerate() {
            by_id
                .entry(hit.message_key().to_string())
                .and_modify(|entry| entry.1 = Some(i + 1))
                .or_insert((None, Some(i + 1), hit));
        }

        let w_sem = weight;
        let w_lex = 1.0 - weight;
        let mut fused: Vec<SearchHit> = by_id
            .into_values()
            .map(|(lex_rank, sem_rank, mut hit)| {
                let lex_term = lex_rank.map_or(0.0, |rank| w_lex / (RRF_K + rrf_rank(rank)));
                let sem_term = sem_rank.map_or(0.0, |rank| w_sem / (RRF_K + rrf_rank(rank)));
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
                .then_with(|| a.session_key().cmp(b.session_key()))
                .then_with(|| a.message_key().cmp(b.message_key()))
        });
        fused.truncate(limit);
        Ok(fused)
    }
}

fn rrf_rank(rank: usize) -> f32 {
    match u16::try_from(rank) {
        Ok(rank) => f32::from(rank),
        Err(_) => f32::from(u16::MAX),
    }
}
