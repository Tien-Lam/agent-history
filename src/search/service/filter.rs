use std::collections::{HashMap, HashSet};

use crate::metadata;
use crate::model::Session;
use crate::session_resolver::{qualified_session_metadata_key, source_for_session};

use super::SearchServiceHit;
use crate::search::HitKind;

pub(super) fn filter_hits_by_metadata(
    hits: Vec<SearchServiceHit>,
    session_meta: &HashMap<String, &Session>,
    source_by_session: &HashMap<String, String>,
    metadata_keys: Option<&HashSet<String>>,
) -> Vec<SearchServiceHit> {
    let Some(keys) = metadata_keys else {
        return hits;
    };
    hits.into_iter()
        .filter(|(hit, _)| match hit.kind {
            HitKind::Message => session_meta
                .get(hit.session_key.as_str())
                .map(|s| {
                    qualified_session_metadata_key(s, source_for_session(source_by_session, s))
                })
                .is_some_and(|k| keys.contains(&k)),
            HitKind::Note => hit
                .note_session_ref
                .as_deref()
                .and_then(|raw| metadata::session_key_from_ref(raw).ok())
                .is_some_and(|k| keys.contains(&k)),
        })
        .collect()
}

pub(super) fn filter_hits_to_current_sessions(
    hits: Vec<SearchServiceHit>,
    session_meta: &HashMap<String, &Session>,
    source_by_session: &HashMap<String, String>,
) -> Vec<SearchServiceHit> {
    let session_refs: HashSet<String> = session_meta
        .values()
        .map(|session| {
            qualified_session_metadata_key(session, source_for_session(source_by_session, session))
        })
        .collect();

    hits.into_iter()
        .filter(|(hit, _)| match hit.kind {
            HitKind::Message => session_meta.contains_key(hit.session_key.as_str()),
            HitKind::Note => hit
                .note_session_ref
                .as_deref()
                .and_then(|raw| metadata::session_key_from_ref(raw).ok())
                .is_some_and(|session_ref| session_refs.contains(&session_ref)),
        })
        .collect()
}

pub(super) fn sort_search_hits(
    hits: &mut [SearchServiceHit],
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
