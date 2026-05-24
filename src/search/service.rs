use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use thiserror::Error;

use crate::action::Action;
use crate::cursor::CursorError;
use crate::metadata;
use crate::model::{Provider, Session};
use crate::provider::HistoryProvider;
use crate::session_resolver::source_for_session;
use crate::session_warnings::SessionLoadWarning;

use super::{Explanation, SearchFilters, SearchHit, SearchIndex};
use crate::search::types::IndexLoadError;

pub(super) mod citation;
pub(super) mod cursor;
mod filter;
mod raw;

use filter::{filter_hits_by_metadata, filter_hits_to_current_sessions, sort_search_hits};
use raw::raw_search_hits;

pub type SearchServiceHit = (SearchHit, Option<Explanation>);

#[derive(Debug, Error)]
pub enum SearchServiceError {
    #[error("failed to open search index: {0}")]
    OpenIndex(#[source] super::SearchError),
    #[error("failed to build search index: {0}")]
    BuildIndex(#[source] super::SearchError),
    #[error("failed to inspect index: {0}")]
    InspectIndex(#[source] super::SearchError),
    #[error("failed to encode search cursor: {0}")]
    Cursor(#[source] CursorError),
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
    pub warnings: Vec<SessionLoadWarning>,
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
        let stats = if let Some(scope) = request.provider_scope {
            index
                .build_index_for_providers(sessions, self.providers, &tx, scope)
                .map_err(SearchServiceError::BuildIndex)?
        } else {
            index
                .build_index(sessions, self.providers, &tx)
                .map_err(SearchServiceError::BuildIndex)?
        };

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
        let warnings = index_load_warnings(&stats.load_errors, &session_meta, source_by_session);
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
            warnings,
        })
    }
}

fn index_load_warnings(
    errors: &[IndexLoadError],
    session_meta: &HashMap<String, &Session>,
    source_by_session: &HashMap<String, String>,
) -> Vec<SessionLoadWarning> {
    errors
        .iter()
        .filter_map(|error| {
            let session = session_meta.get(&error.session_key).copied()?;
            let source = source_for_session(source_by_session, session);
            Some(SessionLoadWarning::new(source, session, &error.error))
        })
        .collect()
}

pub fn index_notes_best_effort(index: &SearchIndex) {
    index_notes_best_effort_for_path(index, metadata::default_path());
}

pub(crate) fn index_notes_best_effort_for_path(index: &SearchIndex, path: Option<PathBuf>) {
    let Some(path) = path else {
        let _ = index.index_notes(&[]);
        return;
    };
    let notes = if path.exists() {
        metadata::open(&path)
            .and_then(|conn| metadata::note_list(&conn, None))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let _ = index.index_notes(&notes);
}
