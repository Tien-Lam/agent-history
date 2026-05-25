use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use crate::cursor::SearchCursor;
use crate::dto::SearchHitJson;
use crate::federated::FederatedDiscovery;
use crate::model::{Provider, Session};
use crate::provider::HistoryProvider;
use crate::search::{
    self, SearchCollection, SearchFilters, SearchHitCitation, SearchService, SearchServiceError,
    SearchServiceHit, SearchServiceRequest,
};
use crate::session_warnings::SessionLoadWarning;

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
    /// Force full result collection even for the first page. Watch mode uses
    /// this with an overfetching page size so dedupe cannot strand unseen hits
    /// behind the public one-shot search limit.
    pub exhaustive: bool,
}

pub struct SearchSessionsPage<'a> {
    pub hits: Vec<SearchServiceHit>,
    pub session_meta: HashMap<String, &'a Session>,
    pub total: usize,
    pub next_cursor: Option<String>,
    pub engine: &'static str,
    pub citations: HashMap<String, SearchHitCitation>,
    pub warnings: Vec<SessionLoadWarning>,
    pub metadata_warnings: Vec<String>,
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
            filters: request.filters,
            debug_search: request.debug_search,
            hybrid_weight: request.hybrid_weight,
            metadata_keys: request.metadata_keys,
            provider_scope: request.provider_scope,
            collection: search_collection(request),
        },
    )?;

    let total = output.total;
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
    let hits = output
        .hits
        .get(page_start..page_end)
        .unwrap_or(&[])
        .to_vec();
    let next_cursor =
        search::next_search_cursor(&hits, page_end < output.hits.len(), &output.session_meta)
            .map_err(SearchServiceError::Cursor)?;
    let citation_resolution = search::resolve_search_hit_citations(
        &hits,
        &output.session_meta,
        &discovery.source_by_session,
        providers,
    );
    let mut warnings = output.warnings;
    warnings.extend(citation_resolution.warnings);

    Ok(SearchSessionsPage {
        hits,
        session_meta: output.session_meta,
        total,
        next_cursor,
        engine: output.engine,
        citations: citation_resolution.refs,
        warnings,
        metadata_warnings: output.metadata_warnings,
    })
}

fn search_collection(request: SearchSessionsRequest<'_>) -> SearchCollection {
    if request.exhaustive {
        return SearchCollection::Full;
    }
    if request.cursor.is_none()
        && request.metadata_keys.is_none()
        && request.filters.project_needle().is_none()
        && request.hybrid_weight <= 0.0
    {
        return SearchCollection::Top(request.limit.saturating_add(1).max(1));
    }
    SearchCollection::Full
}

pub fn search_hit_json<S: BuildHasher>(
    page: &SearchSessionsPage<'_>,
    source_by_session: &HashMap<String, String, S>,
    include_explanations: bool,
) -> Vec<SearchHitJson> {
    page.hits
        .iter()
        .map(|(hit, explanation)| {
            let explanation = include_explanations
                .then_some(explanation.as_ref())
                .flatten();
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
