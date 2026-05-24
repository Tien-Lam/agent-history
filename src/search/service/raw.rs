use std::path::Path;

use crate::embed;

use super::{SearchServiceError, SearchServiceHit};
use crate::search::{SearchFilters, SearchIndex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RawSearchLimit {
    Full,
    Top(usize),
}

pub(super) struct RawSearchOutput {
    pub(super) hits: Vec<SearchServiceHit>,
    pub(super) total: usize,
    pub(super) engine: &'static str,
}

pub(super) fn raw_search_hits(
    index_dir: &Path,
    index: &SearchIndex,
    query: &str,
    limit: RawSearchLimit,
    filters: &SearchFilters,
    debug_search: bool,
    hybrid_weight: f32,
) -> Result<RawSearchOutput, SearchServiceError> {
    let pool_size = match limit {
        RawSearchLimit::Full => index
            .num_docs()
            .map_err(SearchServiceError::InspectIndex)?
            .max(1),
        RawSearchLimit::Top(limit) => limit.max(1),
    };

    let hybrid_hits = if hybrid_weight > 0.0 {
        embed::try_hybrid_search(index_dir, index, query, pool_size, filters, hybrid_weight)
    } else {
        None
    };

    if let Some(hits) = hybrid_hits {
        let rows: Vec<SearchServiceHit> = hits.into_iter().map(|h| (h, None)).collect();
        let total = rows.len();
        return Ok(RawSearchOutput {
            hits: rows,
            total,
            engine: "hybrid",
        });
    }

    let output = index
        .search_inner_with_total(query, pool_size, filters, debug_search)
        .map_err(SearchServiceError::Search)?;
    let hits = if debug_search {
        output
            .hits
            .into_iter()
            .map(|(h, e)| {
                let explanation = e.unwrap_or_else(|| {
                    crate::search::Explanation::new_with_string(
                        "no explanation available".into(),
                        h.score,
                    )
                });
                (h, Some(explanation))
            })
            .collect()
    } else {
        output.hits
    };
    Ok(RawSearchOutput {
        hits,
        total: output.total,
        engine: "lexical",
    })
}
