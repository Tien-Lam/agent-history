use std::path::Path;

use crate::embed;

use super::{SearchServiceError, SearchServiceHit};
use crate::search::{SearchFilters, SearchIndex};

pub(super) fn raw_search_hits(
    index_dir: &Path,
    index: &SearchIndex,
    query: &str,
    pool_size: usize,
    filters: &SearchFilters,
    debug_search: bool,
    hybrid_weight: f32,
) -> Result<(Vec<SearchServiceHit>, &'static str), SearchServiceError> {
    let hybrid_hits = if hybrid_weight > 0.0 {
        embed::try_hybrid_search(index_dir, index, query, pool_size, filters, hybrid_weight)
    } else {
        None
    };

    if let Some(hits) = hybrid_hits {
        let rows = hits.into_iter().map(|h| (h, None)).collect();
        return Ok((rows, "hybrid"));
    }
    if debug_search {
        let hits = index
            .search_with_filters_and_explain(query, pool_size, filters)
            .map_err(SearchServiceError::Search)?
            .into_iter()
            .map(|(h, e)| (h, Some(e)))
            .collect();
        return Ok((hits, "lexical"));
    }
    let hits = index
        .search_with_filters(query, pool_size, filters)
        .map_err(SearchServiceError::Search)?
        .into_iter()
        .map(|h| (h, None))
        .collect();
    Ok((hits, "lexical"))
}
