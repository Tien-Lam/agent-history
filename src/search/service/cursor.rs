use std::collections::HashMap;
use std::hash::BuildHasher;

use crate::model::Session;

use super::SearchServiceHit;
use crate::search::SearchHit;

pub fn next_search_cursor<S: BuildHasher>(
    page: &[SearchServiceHit],
    has_more: bool,
    session_meta: &HashMap<String, &Session, S>,
) -> Option<String> {
    if !has_more {
        return None;
    }
    page.last().map(|(h, _)| {
        crate::cursor::SearchCursor {
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

pub fn search_hit_is_after_cursor<S: BuildHasher>(
    hit: &SearchHit,
    sessions: &HashMap<String, &Session, S>,
    cursor: &crate::cursor::SearchCursor,
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
