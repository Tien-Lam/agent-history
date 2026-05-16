mod input;
mod output;
mod results;
mod watch;

use std::collections::{HashMap, HashSet};
use std::io::{self, IsTerminal};
use std::path::Path;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::{QualifiedCitationRef, Session};
use aghist::search::{self, SearchFilters};
use aghist::{config, federated, provider};

use super::metadata::try_index_notes;
use input::{decode_search_cursor, resolve_nonempty_search_query};
use output::{print_search_json, print_search_table};
use results::{
    filter_hits_by_metadata, filter_hits_to_current_sessions, next_search_cursor, raw_search_hits,
    search_hit_is_after_cursor, sort_search_hits,
};
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

type SearchHitRow = (search::SearchHit, Option<search::Explanation>);

pub(crate) fn search_command(
    providers: &[Box<dyn provider::HistoryProvider>],
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

    let federation = federated_discovery_for_search(providers);
    let sessions: Vec<Session> = federation.sessions;

    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new("index-error", format!("failed to open search index: {e}"))
    })?;

    let (tx, _rx) = crossbeam_channel::unbounded::<aghist::action::Action>();
    index.build_index(&sessions, providers, &tx).map_err(|e| {
        ErrorEnvelope::new("index-error", format!("failed to build search index: {e}"))
    })?;

    try_index_notes(&index);

    let pool_size = index
        .num_docs()
        .map_err(|e| ErrorEnvelope::new("index-error", format!("failed to inspect index: {e}")))?
        .max(limit)
        .max(1);

    let (raw_hits, engine_used) = raw_search_hits(
        &index_dir,
        &index,
        query,
        pool_size,
        filters,
        debug_search,
        hybrid_weight,
    )?;

    let session_meta: HashMap<String, &Session> =
        sessions.iter().map(|s| (s.identity_key(), s)).collect();

    let raw_hits = filter_hits_to_current_sessions(raw_hits, &session_meta);
    let raw_hits = filter_hits_by_metadata(raw_hits, &session_meta, metadata_keys);

    let total = raw_hits.len();
    if raw_hits.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let mut ordered = raw_hits;
    sort_search_hits(&mut ordered, &session_meta);

    let page_start = match &after {
        Some(c) => ordered
            .iter()
            .position(|(h, _)| search_hit_is_after_cursor(h, &session_meta, c))
            .unwrap_or(ordered.len()),
        None => 0,
    };

    let page_end = page_start.saturating_add(limit).min(ordered.len());
    let page = &ordered[page_start..page_end];
    if page.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let next_cursor = next_search_cursor(page, page_end < ordered.len(), &session_meta);
    let hit_refs = resolve_search_hit_refs(
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
        .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}")))?;
    } else {
        print_search_table(
            page,
            &session_meta,
            &federation.source_by_session,
            next_cursor.as_deref(),
        )
        .map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write search output: {e}"))
        })?;
    }

    Ok(EXIT_OK)
}

fn resolve_search_hit_refs(
    hits: &[(search::SearchHit, Option<search::Explanation>)],
    sessions: &HashMap<String, &Session>,
    source_by_session: &HashMap<String, String>,
    providers: &[Box<dyn provider::HistoryProvider>],
) -> HashMap<String, String> {
    let mut refs = HashMap::new();
    let mut seen_sessions = HashSet::new();

    for (hit, _) in hits {
        if !matches!(hit.kind, search::HitKind::Message) {
            continue;
        }
        if !seen_sessions.insert(hit.session_key.as_str()) {
            continue;
        }
        let Some(session) = sessions.get(hit.session_key.as_str()).copied() else {
            continue;
        };
        let Ok(messages) = provider::load_messages_for_session(session, providers) else {
            continue;
        };
        let source = source_by_session
            .get(hit.session_key.as_str())
            .map_or(federated::LOCAL_SOURCE, String::as_str);
        for (i, msg) in messages.iter().enumerate() {
            let message_key = session.message_key(i, &msg.id.0);
            let turn = i + 1;
            refs.insert(message_key, format_search_ref(source, session, turn));
        }
    }

    refs
}

fn format_search_ref(source: &str, session: &Session, turn: usize) -> String {
    let turn = u32::try_from(turn).unwrap_or(u32::MAX);
    let Some(citation) = session.citation_ref(turn) else {
        return format!("{}/{}#{turn}", session.provider.slug(), session.id.0);
    };
    QualifiedCitationRef::new(
        (source != federated::LOCAL_SOURCE).then(|| source.to_string()),
        citation,
    )
    .to_string()
}

pub(crate) fn federated_discovery_for_search(
    providers: &[Box<dyn provider::HistoryProvider>],
) -> federated::FederatedDiscovery {
    let config = match config::Config::resolved_path() {
        Some(path) => config::Config::load_from(&path),
        None => config::Config::default(),
    };
    let enabled = config.enabled_providers();
    let mut result = if let Some(cache_root) = config::sources_cache_root() {
        federated::discover_federated(providers, &config.sources, &cache_root)
    } else {
        federated::discover_federated(providers, &[], std::path::Path::new(""))
    };
    result.retain_providers(&enabled);
    for failure in &result.failures {
        eprintln!("warning: source '{}': {}", failure.source, failure.message);
    }
    result
}
