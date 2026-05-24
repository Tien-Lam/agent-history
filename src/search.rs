pub use tantivy::query::Explanation;

mod document;
mod fields;
mod fingerprint;
mod index;
mod query;
mod service;
mod snippet;
mod storage;
pub mod types;

#[cfg(test)]
mod tests;

pub use index::SearchIndex;
pub use service::citation::{
    resolve_search_hit_citations, SearchHitCitation, SearchHitCitationResolution,
};
pub use service::cursor::{next_search_cursor, search_hit_is_after_cursor};
pub use service::{
    index_notes_best_effort, SearchCollection, SearchService, SearchServiceError, SearchServiceHit,
    SearchServiceOutput, SearchServiceRequest,
};
pub use types::{
    HitKind, IndexStats, NotesIndexStats, SearchError, SearchFilters, SearchHit, SemanticCandidate,
    RRF_K,
};

/// Cosine similarity in [-1, 1]. Returns 0.0 for mismatched / empty / zero-norm
/// vectors — those can't yield a useful hybrid signal anyway.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}
