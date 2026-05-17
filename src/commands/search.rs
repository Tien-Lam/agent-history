mod input;
mod output;
mod watch;

use std::collections::HashSet;
use std::io::{self, IsTerminal};
use std::path::Path;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::search::{self, SearchFilters};
use aghist::{provider, query_scope};

use super::discovery::federated_discovery_for_commands;
use input::{decode_search_cursor, resolve_nonempty_search_query};
use output::{print_search_json, print_search_table};
pub(crate) use watch::{search_watch_command, SearchWatchRequest};

#[derive(Clone, Copy)]
pub(crate) struct SearchCommandRequest<'a> {
    pub(crate) query: Option<&'a str>,
    pub(crate) query_file: Option<&'a Path>,
    pub(crate) stdin: bool,
    pub(crate) limit: usize,
    pub(crate) cursor: Option<&'a str>,
    pub(crate) force_json: bool,
    pub(crate) filters: &'a SearchFilters,
    pub(crate) debug_search: bool,
    pub(crate) hybrid_weight: f32,
    pub(crate) metadata_keys: Option<&'a HashSet<String>>,
}

type SearchHitRow = search::SearchServiceHit;

pub(crate) fn search_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    request: SearchCommandRequest<'_>,
) -> Result<i32, ErrorEnvelope> {
    let SearchCommandRequest {
        query,
        query_file,
        stdin,
        limit,
        cursor,
        force_json,
        filters,
        debug_search,
        hybrid_weight,
        metadata_keys,
    } = request;
    let resolved = match resolve_nonempty_search_query(query, query_file, stdin) {
        Ok(q) => q,
        Err(exit) => return Ok(exit),
    };
    let query = resolved.as_str();

    let after = match decode_search_cursor(cursor) {
        Ok(c) => c,
        Err(exit) => return Ok(exit),
    };

    let federation = federated_discovery_for_commands(providers, scope);
    let service = search::SearchService::new(providers);
    let search::SearchServiceOutput {
        hits: ordered,
        session_meta,
        engine: engine_used,
    } = service
        .search(
            &federation.sessions,
            &federation.source_by_session,
            search::SearchServiceRequest {
                query,
                limit,
                filters,
                debug_search,
                hybrid_weight,
                metadata_keys,
                provider_scope: None,
            },
        )
        .map_err(|e| ErrorEnvelope::new("index-error", e.to_string()))?;

    let total = ordered.len();
    if ordered.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let page_start = match &after {
        Some(c) => ordered
            .iter()
            .position(|(h, _)| search::search_hit_is_after_cursor(h, &session_meta, c))
            .unwrap_or(ordered.len()),
        None => 0,
    };

    let page_end = page_start.saturating_add(limit).min(ordered.len());
    let page = &ordered[page_start..page_end];
    if page.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let next_cursor = search::next_search_cursor(page, page_end < ordered.len(), &session_meta);
    let hit_refs = search::resolve_search_hit_citations(
        page,
        &session_meta,
        &federation.source_by_session,
        providers,
    );

    let want_json = force_json || !io::stdout().is_terminal();
    if want_json {
        print_search_json(
            page,
            &session_meta,
            &federation.source_by_session,
            &hit_refs,
            total,
            next_cursor.as_deref(),
            engine_used,
        )
        .map_err(|e| ErrorEnvelope::io("failed to write JSON output", e))?;
    } else {
        print_search_table(
            page,
            &session_meta,
            &federation.source_by_session,
            next_cursor.as_deref(),
        )
        .map_err(|e| ErrorEnvelope::io("failed to write search output", e))?;
    }

    Ok(EXIT_OK)
}
