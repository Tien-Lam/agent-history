use std::collections::{HashMap, HashSet};

use crate::cursor::SearchCursor;
use crate::dto::SearchHitJson;
use crate::federated::FederatedDiscovery;
use crate::model::{Provider, Session};
use crate::provider::HistoryProvider;
use crate::search::{
    self, SearchFilters, SearchHitCitation, SearchService, SearchServiceError, SearchServiceHit,
    SearchServiceRequest,
};

#[derive(Clone, Copy)]
pub struct SearchSessionsRequest<'a> {
    pub query: &'a str,
    pub limit: usize,
    pub cursor: Option<&'a SearchCursor>,
    pub filters: &'a SearchFilters,
    pub debug_search: bool,
    pub hybrid_weight: f32,
    pub metadata_keys: Option<&'a HashSet<String>>,
    pub provider_scope: Option<&'a HashSet<Provider>>,
}

pub struct SearchSessionsPage<'a> {
    pub hits: Vec<SearchServiceHit>,
    pub session_meta: HashMap<String, &'a Session>,
    pub total: usize,
    pub next_cursor: Option<String>,
    pub engine: &'static str,
    pub citations: HashMap<String, SearchHitCitation>,
}

pub fn search_sessions<'a>(
    providers: &[Box<dyn HistoryProvider>],
    discovery: &'a FederatedDiscovery,
    request: SearchSessionsRequest<'_>,
) -> Result<SearchSessionsPage<'a>, SearchServiceError> {
    let output = SearchService::new(providers).search(
        &discovery.sessions,
        &discovery.source_by_session,
        SearchServiceRequest {
            query: request.query,
            limit: request.limit,
            filters: request.filters,
            debug_search: request.debug_search,
            hybrid_weight: request.hybrid_weight,
            metadata_keys: request.metadata_keys,
            provider_scope: request.provider_scope,
        },
    )?;

    let total = output.hits.len();
    let page_start = match request.cursor {
        Some(cursor) => output
            .hits
            .iter()
            .position(|(hit, _)| {
                search::search_hit_is_after_cursor(hit, &output.session_meta, cursor)
            })
            .unwrap_or(output.hits.len()),
        None => 0,
    };
    let page_end = page_start
        .saturating_add(request.limit)
        .min(output.hits.len());
    let hits = output.hits[page_start..page_end].to_vec();
    let next_cursor =
        search::next_search_cursor(&hits, page_end < output.hits.len(), &output.session_meta);
    let citations = search::resolve_search_hit_citations(
        &hits,
        &output.session_meta,
        &discovery.source_by_session,
        providers,
    );

    Ok(SearchSessionsPage {
        hits,
        session_meta: output.session_meta,
        total,
        next_cursor,
        engine: output.engine,
        citations,
    })
}

pub fn search_hit_json(
    page: &SearchSessionsPage<'_>,
    source_by_session: &HashMap<String, String>,
    include_explanations: bool,
) -> Vec<SearchHitJson> {
    page.hits
        .iter()
        .map(|(hit, explanation)| {
            let explanation = include_explanations.then(|| explanation.as_ref()).flatten();
            SearchHitJson::from_search_hit(
                hit,
                explanation,
                &page.session_meta,
                source_by_session,
                Some(&page.citations),
            )
        })
        .collect()
}
