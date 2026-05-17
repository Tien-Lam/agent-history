use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::action::Action;
use crate::embed;
use crate::federated::LOCAL_SOURCE;
use crate::metadata;
use crate::model::{Provider, QualifiedCitationRef, Session};
use crate::provider::{self, HistoryProvider};

use super::{Explanation, HitKind, SearchFilters, SearchHit, SearchIndex};

pub type SearchServiceHit = (SearchHit, Option<Explanation>);

#[derive(Debug, Error)]
pub enum SearchServiceError {
    #[error("failed to open search index: {0}")]
    OpenIndex(#[source] super::SearchError),
    #[error("failed to build search index: {0}")]
    BuildIndex(#[source] super::SearchError),
    #[error("failed to inspect index: {0}")]
    InspectIndex(#[source] super::SearchError),
    #[error("search failed: {0}")]
    Search(#[source] super::SearchError),
}

pub struct SearchService<'a> {
    providers: &'a [Box<dyn HistoryProvider>],
    index_dir: PathBuf,
}

#[derive(Clone, Copy)]
pub struct SearchServiceRequest<'a> {
    pub query: &'a str,
    pub limit: usize,
    pub filters: &'a SearchFilters,
    pub debug_search: bool,
    pub hybrid_weight: f32,
    pub metadata_keys: Option<&'a HashSet<String>>,
    pub provider_scope: Option<&'a HashSet<Provider>>,
}

pub struct SearchServiceOutput<'a> {
    pub hits: Vec<SearchServiceHit>,
    pub session_meta: HashMap<String, &'a Session>,
    pub engine: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHitCitation {
    pub ref_: String,
    pub turn: usize,
}

impl<'a> SearchService<'a> {
    pub fn new(providers: &'a [Box<dyn HistoryProvider>]) -> Self {
        Self {
            providers,
            index_dir: SearchIndex::default_index_dir(),
        }
    }

    pub fn with_index_dir(
        providers: &'a [Box<dyn HistoryProvider>],
        index_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            providers,
            index_dir: index_dir.into(),
        }
    }

    pub fn search<'s>(
        &self,
        sessions: &'s [Session],
        source_by_session: &HashMap<String, String>,
        request: SearchServiceRequest<'_>,
    ) -> Result<SearchServiceOutput<'s>, SearchServiceError> {
        let index =
            SearchIndex::open_or_create(&self.index_dir).map_err(SearchServiceError::OpenIndex)?;

        let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
        if let Some(scope) = request.provider_scope {
            index
                .build_index_for_providers(sessions, self.providers, &tx, scope)
                .map_err(SearchServiceError::BuildIndex)?;
        } else {
            index
                .build_index(sessions, self.providers, &tx)
                .map_err(SearchServiceError::BuildIndex)?;
        }

        index_notes_best_effort(&index);

        let pool_size = index
            .num_docs()
            .map_err(SearchServiceError::InspectIndex)?
            .max(request.limit)
            .max(1);

        let (raw_hits, engine) = raw_search_hits(
            self.index_dir.as_path(),
            &index,
            request.query,
            pool_size,
            request.filters,
            request.debug_search,
            request.hybrid_weight,
        )?;

        let session_meta: HashMap<String, &Session> =
            sessions.iter().map(|s| (s.identity_key(), s)).collect();
        let mut hits = filter_hits_to_current_sessions(raw_hits, &session_meta, source_by_session);
        hits = filter_hits_by_metadata(
            hits,
            &session_meta,
            source_by_session,
            request.metadata_keys,
        );
        sort_search_hits(&mut hits, &session_meta);

        Ok(SearchServiceOutput {
            hits,
            session_meta,
            engine,
        })
    }
}

pub fn index_notes_best_effort(index: &SearchIndex) {
    let Some(path) = metadata::default_path() else {
        return;
    };
    if !path.exists() {
        return;
    }
    let Ok(conn) = metadata::open(&path) else {
        return;
    };
    let Ok(notes) = metadata::note_list(&conn, None) else {
        return;
    };
    let _ = index.index_notes(&notes);
}

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

pub fn resolve_search_hit_citations<SessionHasher: BuildHasher, SourceHasher: BuildHasher>(
    hits: &[SearchServiceHit],
    sessions: &HashMap<String, &Session, SessionHasher>,
    source_by_session: &HashMap<String, String, SourceHasher>,
    providers: &[Box<dyn HistoryProvider>],
) -> HashMap<String, SearchHitCitation> {
    let mut refs = HashMap::new();
    let mut seen_sessions = HashSet::new();

    for (hit, _) in hits {
        if !matches!(hit.kind, HitKind::Message) {
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
        let source = source_for_session(source_by_session, session);
        for (i, msg) in messages.iter().enumerate() {
            let message_key = session.message_key(i, &msg.id.0);
            let turn = i + 1;
            refs.insert(
                message_key,
                SearchHitCitation {
                    ref_: format_search_ref(source, session, turn),
                    turn,
                },
            );
        }
    }

    refs
}

fn raw_search_hits(
    index_dir: &Path,
    index: &SearchIndex,
    query: &str,
    pool_size: usize,
    filters: &SearchFilters,
    debug_search: bool,
    hybrid_weight: f32,
) -> Result<(Vec<SearchServiceHit>, &'static str), SearchServiceError> {
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
            .map_err(SearchServiceError::Search)?
            .into_iter()
            .map(|(h, e)| (h, Some(e)))
            .collect();
        return Ok((hits, "lexical"));
    }
    let hits = index
        .search_with_filters(query, pool_size, filters)
        .map_err(SearchServiceError::Search)?
        .into_iter()
        .map(|h| (h, None))
        .collect();
    Ok((hits, "lexical"))
}

fn filter_hits_by_metadata(
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

fn filter_hits_to_current_sessions(
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

fn sort_search_hits(hits: &mut [SearchServiceHit], session_meta: &HashMap<String, &Session>) {
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

fn source_for_session<'a, S: BuildHasher>(
    source_by_session: &'a HashMap<String, String, S>,
    session: &Session,
) -> &'a str {
    source_by_session
        .get(session.identity_key().as_str())
        .map_or(LOCAL_SOURCE, String::as_str)
}

fn qualified_session_metadata_key(session: &Session, source: &str) -> String {
    let raw = session.session_ref().to_string();
    if source == LOCAL_SOURCE {
        raw
    } else {
        format!("{source}:{raw}")
    }
}

fn format_search_ref(source: &str, session: &Session, turn: usize) -> String {
    let turn = u32::try_from(turn).unwrap_or(u32::MAX);
    let Some(citation) = session.citation_ref(turn) else {
        return format!("{}/{}#{turn}", session.provider.slug(), session.id.0);
    };
    QualifiedCitationRef::new(
        (source != LOCAL_SOURCE).then(|| source.to_string()),
        citation,
    )
    .to_string()
}
