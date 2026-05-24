use std::path::Path;

use super::{Consent, EmbeddingStore};

#[cfg(feature = "embeddings")]
use super::Embedder;

/// Whether semantic / hybrid search is wired up for `index_dir`. Returns
/// `true` only when (a) the binary was built with the `embeddings` feature,
/// (b) the user has recorded model-download consent, and (c) the embedding
/// sidecar exists and is non-empty. Used by the TUI to decide whether to
/// expose the hybrid toggle.
pub fn hybrid_ready(index_dir: &Path) -> bool {
    if !cfg!(feature = "embeddings") {
        return false;
    }
    if Consent::load(index_dir).is_none() {
        return false;
    }
    matches!(EmbeddingStore::open(index_dir), Ok(Some(s)) if !s.is_empty())
}

/// Run a hybrid (lexical + semantic) RRF search end-to-end. Embeds `query`,
/// ranks the on-disk embedding store by cosine similarity, then fuses the
/// top-N candidates with the lexical results inside `index`.
///
/// Fail-open: returns `None` whenever the semantic side isn't ready (missing
/// `embeddings` feature, no consent, empty store, embedder init failure,
/// query embedding failure, or the fused index search failed). Callers should
/// fall back to lexical-only on `None`.
#[cfg(feature = "embeddings")]
pub fn try_hybrid_search(
    index_dir: &Path,
    index: &crate::search::SearchIndex,
    query: &str,
    pool_size: usize,
    filters: &crate::search::SearchFilters,
    hybrid_weight: f32,
) -> Option<Vec<crate::search::SearchHit>> {
    use crate::search;

    Consent::load(index_dir)?;
    let store = match EmbeddingStore::open(index_dir) {
        Ok(Some(s)) if !s.is_empty() => s,
        _ => return None,
    };
    let cache_dir = index_dir.join("models");
    let mut embedder = Embedder::try_new(&cache_dir).ok()?;
    let q_vec = match embedder.embed_batch(&[query.to_string()]) {
        Ok(mut v) if !v.is_empty() => v.swap_remove(0),
        _ => return None,
    };

    let mut ranked: Vec<(String, f32)> = store
        .iter()
        .map(|(id, vec)| (id.to_string(), search::cosine_similarity(&q_vec, vec)))
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(pool_size);
    let candidates: Vec<search::SemanticCandidate> = ranked
        .into_iter()
        .map(|(message_key, similarity)| search::SemanticCandidate {
            message_key,
            message_id: String::new(),
            similarity,
        })
        .collect();

    index
        .search_hybrid(
            query,
            &candidates,
            pool_size,
            filters,
            hybrid_weight,
            pool_size,
        )
        .ok()
}

#[cfg(not(feature = "embeddings"))]
pub const fn try_hybrid_search(
    _index_dir: &Path,
    _index: &crate::search::SearchIndex,
    _query: &str,
    _pool_size: usize,
    _filters: &crate::search::SearchFilters,
    _hybrid_weight: f32,
) -> Option<Vec<crate::search::SearchHit>> {
    None
}
