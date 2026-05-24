use std::collections::HashSet;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::search::SearchFilters;
use aghist::services::search as search_service;
use aghist::{provider, query_scope};

use super::super::discovery::federated_discovery_for_commands;
use super::input::resolve_search_query;
use super::output::write_watch_hit;

pub(crate) fn search_watch_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    request: SearchWatchRequest<'_>,
) -> Result<i32, ErrorEnvelope> {
    let SearchWatchRequest {
        query,
        query_file,
        stdin,
        limit,
        interval_ms,
        max_iterations,
        filters,
        metadata_keys,
    } = request;
    let resolved = resolve_search_query(query, query_file, stdin)?;
    let query = resolved.as_str();
    if query.trim().is_empty() {
        return Err(ErrorEnvelope::new("usage", "search query is empty")
            .with_hint("Run `aghist search --help` for usage."));
    }

    let emit_limit = limit.max(1);
    let interval = Duration::from_millis(interval_ms);
    let mut seen: HashSet<String> = HashSet::new();
    let mut iteration: u32 = 0;
    let stdout = io::stdout();

    loop {
        iteration += 1;

        let federation = federated_discovery_for_commands(providers, scope);
        let page = search_service::search_sessions(
            providers,
            &federation,
            search_service::SearchSessionsRequest {
                query,
                limit: watch_candidate_limit(emit_limit, seen.len()),
                cursor: None,
                filters,
                debug_search: false,
                hybrid_weight: 0.0,
                metadata_keys,
                provider_scope: Some(scope.providers()),
            },
        )
        .map_err(|e| ErrorEnvelope::new("index-error", e.to_string()))?;
        for warning in &page.warnings {
            eprintln!("{}", warning.warning_line());
        }
        for warning in &page.metadata_warnings {
            eprintln!("warning: metadata: {warning}");
        }

        let mut emitted_this_poll = 0;
        let mut handle = stdout.lock();
        for (h, _) in &page.hits {
            if !seen.insert(h.message_key().to_string()) {
                continue;
            }
            if write_watch_hit(
                &mut handle,
                h,
                &page.session_meta,
                &federation.source_by_session,
            )
            .is_err()
            {
                return Ok(EXIT_OK);
            }
            emitted_this_poll += 1;
            if emitted_this_poll >= emit_limit {
                break;
            }
        }
        if handle.flush().is_err() {
            return Ok(EXIT_OK);
        }
        drop(handle);

        if max_iterations > 0 && iteration >= max_iterations {
            return Ok(EXIT_OK);
        }

        std::thread::sleep(interval);
    }
}

fn watch_candidate_limit(page_limit: usize, seen_count: usize) -> usize {
    page_limit.saturating_add(seen_count).max(1)
}

#[derive(Clone, Copy)]
pub(crate) struct SearchWatchRequest<'a> {
    pub(crate) query: Option<&'a str>,
    pub(crate) query_file: Option<&'a Path>,
    pub(crate) stdin: bool,
    pub(crate) limit: usize,
    pub(crate) interval_ms: u64,
    pub(crate) max_iterations: u32,
    pub(crate) filters: &'a SearchFilters,
    pub(crate) metadata_keys: Option<&'a HashSet<String>>,
}
