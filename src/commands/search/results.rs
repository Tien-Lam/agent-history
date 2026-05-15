use std::collections::{HashMap, HashSet};
use std::path::Path;

use aghist::cli_error::ErrorEnvelope;
use aghist::embed;
use aghist::model::Session;
use aghist::search::{self, SearchFilters};

use super::super::filtering::{session_metadata_key, strip_turn_suffix};
use super::SearchHitRow;

pub(super) fn raw_search_hits(
    index_dir: &Path,
    index: &search::SearchIndex,
    query: &str,
    pool_size: usize,
    filters: &SearchFilters,
    debug_search: bool,
    hybrid_weight: f32,
) -> Result<(Vec<SearchHitRow>, &'static str), ErrorEnvelope> {
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

pub(super) fn filter_hits_by_metadata(
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

pub(super) fn sort_search_hits(
    hits: &mut [SearchHitRow],
    session_meta: &HashMap<String, &Session>,
) {
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

pub(super) fn next_search_cursor(
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

pub(super) fn search_hit_is_after_cursor(
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
