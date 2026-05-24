use std::collections::HashSet;

use thiserror::Error;

use crate::cursor::ListCursor;
use crate::federated::FederatedDiscovery;
use crate::model::Session;
use crate::provider::HistoryProvider;
use crate::search::SearchFilters;
use crate::session_resolver::{metadata_filter_matches_source, source_for_session};
use crate::session_warnings::SessionLoadWarning;

mod filters;
mod labels;
mod paging;
#[cfg(test)]
mod tests;

use filters::session_has_matching_message;
pub use labels::{source_provider_counts, source_provider_label};
use paging::{compare_listed_sessions, listed_session_is_after_cursor};

#[derive(Clone, Copy)]
pub struct ListSessionsRequest<'a> {
    pub limit: usize,
    pub cursor: Option<&'a str>,
    pub filters: &'a SearchFilters,
    pub metadata_keys: Option<&'a HashSet<String>>,
}

pub struct ListSessionsPage {
    pub sessions: Vec<ListedSession>,
    pub total: usize,
    pub next_cursor: Option<String>,
    pub provider_counts: Vec<(String, usize)>,
    pub warnings: Vec<SessionLoadWarning>,
}

#[derive(Clone)]
pub struct ListedSession {
    pub source: String,
    pub session: Session,
}

#[derive(Debug, Error)]
pub enum ListSessionsError {
    #[error("invalid --cursor token")]
    InvalidCursor,
}

pub fn list_sessions_page(
    providers: &[Box<dyn HistoryProvider>],
    discovery: FederatedDiscovery,
    request: ListSessionsRequest<'_>,
) -> Result<ListSessionsPage, ListSessionsError> {
    let after = request
        .cursor
        .map(ListCursor::decode)
        .transpose()
        .map_err(|_| ListSessionsError::InvalidCursor)?;

    let project_needle = request.filters.project_needle();
    let needs_messages = request.filters.needs_message_scan();
    let source_by_session = discovery.source_by_session;
    let mut sessions = Vec::new();
    let mut warnings = Vec::new();

    for session in discovery.sessions {
        if !request
            .filters
            .matches_session_with_project_needle(&session, project_needle.as_deref())
        {
            continue;
        }
        let source = source_for_session(&source_by_session, &session).to_string();
        if !metadata_filter_matches_source(&session, &source, request.metadata_keys) {
            continue;
        }
        if needs_messages {
            match session_has_matching_message(providers, &session, request.filters) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    warnings.push(SessionLoadWarning::new(&source, &session, error));
                    continue;
                }
            }
        }
        sessions.push(ListedSession { source, session });
    }

    sessions.sort_by(compare_listed_sessions);
    let total = sessions.len();
    let provider_counts = source_provider_counts(&sessions);
    let page_start = match &after {
        Some(cursor) => sessions
            .iter()
            .position(|listed| listed_session_is_after_cursor(listed, cursor))
            .unwrap_or(sessions.len()),
        None => 0,
    };
    let page_end = page_start.saturating_add(request.limit).min(sessions.len());
    let page = sessions[page_start..page_end].to_vec();
    let next_cursor = if page_end < sessions.len() {
        page.last().map(|listed| {
            ListCursor {
                started_at: listed.session.started_at,
                session_id: listed.session.id.0.clone(),
                session_key: listed.session.identity_key(),
            }
            .encode()
        })
    } else {
        None
    };

    Ok(ListSessionsPage {
        sessions: page,
        total,
        next_cursor,
        provider_counts,
        warnings,
    })
}
