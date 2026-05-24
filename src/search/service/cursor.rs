use std::cmp::Ordering;
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
            started_at: session_meta.get(h.session_key()).map(|s| s.started_at),
            session_key: h.session_key().to_string(),
            session_id: h.session_id().to_string(),
            message_key: h.message_key().to_string(),
            message_id: h.message_id().to_string(),
            kind: h.kind(),
            note_id: h.note_id(),
        }
        .encode()
    })
}

pub fn search_hit_is_after_cursor<S: BuildHasher>(
    hit: &SearchHit,
    sessions: &HashMap<String, &Session, S>,
    cursor: &crate::cursor::SearchCursor,
) -> bool {
    match hit.score.total_cmp(&cursor.score) {
        Ordering::Less => return true,
        Ordering::Greater => return false,
        Ordering::Equal => {}
    }

    let hit_started = sessions.get(hit.session_key()).map(|s| s.started_at);
    if hit_started != cursor.started_at {
        return cursor.started_at.cmp(&hit_started).is_gt();
    }

    let hit_session_key = if hit.session_key().is_empty() {
        hit.session_id()
    } else {
        hit.session_key()
    };
    let cursor_session_key = if cursor.session_key.is_empty() {
        cursor.session_id.as_str()
    } else {
        cursor.session_key.as_str()
    };
    if hit_session_key != cursor_session_key {
        return hit_session_key > cursor_session_key;
    }

    let hit_message_key = if hit.message_key().is_empty() {
        hit.message_id()
    } else {
        hit.message_key()
    };
    let cursor_message_key = if cursor.message_key.is_empty() {
        cursor.message_id.as_str()
    } else {
        cursor.message_key.as_str()
    };
    if hit_message_key != cursor_message_key {
        return hit_message_key > cursor_message_key;
    }

    let hit_kind = hit.kind();
    if hit_kind != cursor.kind {
        return hit_kind.slug() > cursor.kind.slug();
    }

    hit.note_id() > cursor.note_id
}
