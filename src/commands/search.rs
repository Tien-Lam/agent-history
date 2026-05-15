mod input;
mod output;
mod watch;

use std::collections::{HashMap, HashSet};
use std::io::{self, IsTerminal};
use std::path::Path;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
#[cfg(feature = "embeddings")]
use aghist::embed;
use aghist::model::{QualifiedCitationRef, Session};
use aghist::search::{self, SearchFilters};
use aghist::{config, federated, provider};

use super::filtering::{session_metadata_key, strip_turn_suffix};
use super::metadata::try_index_notes;
use input::{decode_search_cursor, resolve_nonempty_search_query};
use output::{print_search_json, print_search_table};
pub(crate) use watch::{search_watch_command, SearchWatchRequest};

#[cfg(feature = "embeddings")]
fn try_hybrid_search(
    index_dir: &std::path::Path,
    index: &search::SearchIndex,
    query: &str,
    pool_size: usize,
    filters: &SearchFilters,
    hybrid_weight: f32,
) -> Result<Option<Vec<search::SearchHit>>, ErrorEnvelope> {
    if embed::Consent::load(index_dir).is_none() {
        return Ok(None);
    }
    let store = match embed::EmbeddingStore::open(index_dir) {
        Ok(Some(s)) if !s.is_empty() => s,
        _ => return Ok(None),
    };
    let cache_dir = index_dir.join("models");
    let Ok(mut embedder) = embed::Embedder::try_new(&cache_dir) else {
        return Ok(None);
    };
    let q_vec = match embedder.embed_batch(&[query.to_string()]) {
        Ok(mut v) if !v.is_empty() => v.swap_remove(0),
        _ => return Ok(None),
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

    let hits = index
        .search_hybrid(
            query,
            &candidates,
            pool_size,
            filters,
            hybrid_weight,
            pool_size,
        )
        .map_err(|e| ErrorEnvelope::new("index-error", format!("hybrid search failed: {e}")))?;
    Ok(Some(hits))
}

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
        );
    }

    Ok(EXIT_OK)
}

fn raw_search_hits(
    index_dir: &Path,
    index: &search::SearchIndex,
    query: &str,
    pool_size: usize,
    filters: &SearchFilters,
    debug_search: bool,
    hybrid_weight: f32,
) -> Result<(Vec<SearchHitRow>, &'static str), ErrorEnvelope> {
    #[cfg(feature = "embeddings")]
    let hybrid_hits: Option<Vec<search::SearchHit>> = if hybrid_weight > 0.0 {
        try_hybrid_search(index_dir, index, query, pool_size, filters, hybrid_weight)?
    } else {
        None
    };
    #[cfg(not(feature = "embeddings"))]
    let hybrid_hits: Option<Vec<search::SearchHit>> = {
        let _ = (index_dir, hybrid_weight);
        None
    };

    if let Some(hits) = hybrid_hits {
        let rows = hits.into_iter().map(|h| (h, None)).collect();
        return Ok((rows, "hybrid"));
    }
    if debug_search {
        let hits = index
            .search_with_filters_and_explain(query, pool_size, filters)
            .map_err(|e| ErrorEnvelope::new("index-error", format!("search failed: {e}")))?
            .into_iter()
            .map(|(h, e)| (h, Some(e)))
            .collect();
        return Ok((hits, "lexical"));
    }
    let hits = index
        .search_with_filters(query, pool_size, filters)
        .map_err(|e| ErrorEnvelope::new("index-error", format!("search failed: {e}")))?
        .into_iter()
        .map(|h| (h, None))
        .collect();
    Ok((hits, "lexical"))
}

fn filter_hits_by_metadata(
    hits: Vec<SearchHitRow>,
    session_meta: &HashMap<String, &Session>,
    metadata_keys: Option<&HashSet<String>>,
) -> Vec<SearchHitRow> {
    let Some(keys) = metadata_keys else {
        return hits;
    };
    hits.into_iter()
        .filter(|(hit, _)| match hit.kind {
            search::HitKind::Message => session_meta
                .get(hit.session_key.as_str())
                .map(|s| session_metadata_key(s))
                .is_some_and(|k| keys.contains(&k)),
            search::HitKind::Note => hit
                .note_session_ref
                .as_deref()
                .map(strip_turn_suffix)
                .is_some_and(|k| keys.contains(k)),
        })
        .collect()
}

fn sort_search_hits(hits: &mut [SearchHitRow], session_meta: &HashMap<String, &Session>) {
    hits.sort_by(|a, b| {
        b.0.score
            .partial_cmp(&a.0.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_started = session_meta
                    .get(a.0.session_key.as_str())
                    .map(|s| s.started_at);
                let b_started = session_meta
                    .get(b.0.session_key.as_str())
                    .map(|s| s.started_at);
                b_started.cmp(&a_started)
            })
            .then_with(|| a.0.session_key.cmp(&b.0.session_key))
            .then_with(|| a.0.message_key.cmp(&b.0.message_key))
            .then_with(|| a.0.kind.slug().cmp(b.0.kind.slug()))
            .then_with(|| a.0.note_id.cmp(&b.0.note_id))
    });
}

fn next_search_cursor(
    page: &[SearchHitRow],
    has_more: bool,
    session_meta: &HashMap<String, &Session>,
) -> Option<String> {
    if !has_more {
        return None;
    }
    page.last().map(|(h, _)| {
        aghist::cursor::SearchCursor {
            score: h.score,
            started_at: session_meta
                .get(h.session_key.as_str())
                .map(|s| s.started_at),
            session_key: h.session_key.clone(),
            session_id: h.session_id.clone(),
            message_key: h.message_key.clone(),
            message_id: h.message_id.clone(),
            kind: h.kind.slug().to_string(),
            note_id: h.note_id,
        }
        .encode()
    })
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

fn search_hit_is_after_cursor(
    hit: &search::SearchHit,
    sessions: &HashMap<String, &Session>,
    cursor: &aghist::cursor::SearchCursor,
) -> bool {
    #[allow(clippy::float_cmp)]
    if hit.score != cursor.score {
        return hit.score < cursor.score;
    }

    let hit_started = sessions.get(hit.session_key.as_str()).map(|s| s.started_at);
    if hit_started != cursor.started_at {
        return cursor.started_at.cmp(&hit_started).is_gt();
    }

    let hit_session_key = if hit.session_key.is_empty() {
        hit.session_id.as_str()
    } else {
        hit.session_key.as_str()
    };
    let cursor_session_key = if cursor.session_key.is_empty() {
        cursor.session_id.as_str()
    } else {
        cursor.session_key.as_str()
    };
    if hit_session_key != cursor_session_key {
        return hit_session_key > cursor_session_key;
    }

    let hit_message_key = if hit.message_key.is_empty() {
        hit.message_id.as_str()
    } else {
        hit.message_key.as_str()
    };
    let cursor_message_key = if cursor.message_key.is_empty() {
        cursor.message_id.as_str()
    } else {
        cursor.message_key.as_str()
    };
    if hit_message_key != cursor_message_key {
        return hit_message_key > cursor_message_key;
    }

    let hit_kind = hit.kind.slug();
    if hit_kind != cursor.kind {
        return hit_kind > cursor.kind.as_str();
    }

    hit.note_id > cursor.note_id
}

pub(crate) fn federated_discovery_for_search(
    providers: &[Box<dyn provider::HistoryProvider>],
) -> federated::FederatedDiscovery {
    let sources = match config::Config::resolved_path() {
        Some(path) => config::Config::load_from(&path).sources,
        None => Vec::new(),
    };
    let Some(cache_root) = config::sources_cache_root() else {
        return federated::discover_federated(providers, &[], std::path::Path::new(""));
    };
    let result = federated::discover_federated(providers, &sources, &cache_root);
    for failure in &result.failures {
        eprintln!("warning: source '{}': {}", failure.source, failure.message);
    }
    result
}
