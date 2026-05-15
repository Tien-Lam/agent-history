use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK, EXIT_USAGE};
use aghist::model::Session;
use aghist::provider;
use aghist::search::{self, SearchFilters};

use super::super::filtering::session_metadata_key;
use super::super::metadata::try_index_notes;
use super::federated_discovery_for_search;
use super::input::resolve_search_query;
use super::output::write_watch_hit;

pub(crate) fn search_watch_command(
    providers: &[Box<dyn provider::HistoryProvider>],
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
    let resolved = match resolve_search_query(query, query_file, stdin) {
        Ok(q) => q,
        Err(env) => {
            env.emit();
            return Ok(EXIT_USAGE);
        }
    };
    let query = resolved.as_str();
    if query.trim().is_empty() {
        ErrorEnvelope::new("usage", "search query is empty")
            .with_hint("Run `aghist search --help` for usage.")
            .emit();
        return Ok(EXIT_USAGE);
    }

    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new("index-error", format!("failed to open search index: {e}"))
    })?;

    let interval = Duration::from_millis(interval_ms);
    let mut seen: HashSet<String> = HashSet::new();
    let mut iteration: u32 = 0;
    let stdout = io::stdout();

    loop {
        iteration += 1;

        let federation = federated_discovery_for_search(providers);
        let sessions: Vec<Session> = federation.sessions;

        let (tx, _rx) = crossbeam_channel::unbounded::<aghist::action::Action>();
        index.build_index(&sessions, providers, &tx).map_err(|e| {
            ErrorEnvelope::new("index-error", format!("failed to build search index: {e}"))
        })?;

        try_index_notes(&index);

        let hits = index
            .search_with_filters(query, limit, filters)
            .map_err(|e| ErrorEnvelope::new("index-error", format!("search failed: {e}")))?;

        let session_meta: HashMap<String, &Session> =
            sessions.iter().map(|s| (s.identity_key(), s)).collect();

        let mut handle = stdout.lock();
        for h in &hits {
            if let Some(keys) = metadata_keys {
                let allowed = session_meta
                    .get(h.session_key.as_str())
                    .map(|s| session_metadata_key(s))
                    .is_some_and(|k| keys.contains(&k));
                if !allowed {
                    continue;
                }
            }
            if !seen.insert(h.message_key.clone()) {
                continue;
            }
            if write_watch_hit(&mut handle, h, &session_meta, &federation.source_by_session)
                .is_err()
            {
                return Ok(EXIT_OK);
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
