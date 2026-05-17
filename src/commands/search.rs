mod input;
mod output;
mod watch;

use std::collections::HashSet;
use std::io::{self, IsTerminal};
use std::path::Path;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::search::{self, SearchFilters};
use aghist::services::search as search_service;
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
    let page = search_service::search_sessions(
        providers,
        &federation,
        search_service::SearchSessionsRequest {
            query,
            limit,
            cursor: after.as_ref(),
            filters,
            debug_search,
            hybrid_weight,
            metadata_keys,
            provider_scope: None,
        },
    )
    .map_err(|e| ErrorEnvelope::new("index-error", e.to_string()))?;

    if page.hits.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    if want_json {
        print_search_json(
            &page.hits,
            &page.session_meta,
            &federation.source_by_session,
            &page.citations,
            page.total,
            page.next_cursor.as_deref(),
            page.engine,
        )
        .map_err(|e| ErrorEnvelope::io("failed to write JSON output", e))?;
    } else {
        print_search_table(
            &page.hits,
            &page.session_meta,
            &federation.source_by_session,
            page.next_cursor.as_deref(),
        )
        .map_err(|e| ErrorEnvelope::io("failed to write search output", e))?;
    }

    Ok(EXIT_OK)
}
