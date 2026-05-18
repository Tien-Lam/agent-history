use std::collections::HashSet;

use thiserror::Error;

use crate::cursor::ListCursor;
use crate::federated::{FederatedDiscovery, LOCAL_SOURCE};
use crate::model::{ContentBlock, Provider, Session};
use crate::provider::{self, HistoryProvider};
use crate::search::SearchFilters;
use crate::session_warnings::SessionLoadWarning;

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

    let project_needle = request
        .filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());
    let needs_messages = request.filters.role.is_some() || request.filters.has_tool_call;
    let source_by_session = discovery.source_by_session;
    let mut sessions = Vec::new();
    let mut warnings = Vec::new();

    for session in discovery.sessions {
        if !session_matches(&session, request.filters, project_needle.as_deref()) {
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

pub fn source_provider_label(source: &str, provider: Provider) -> String {
    if source == LOCAL_SOURCE {
        provider.to_string()
    } else {
        format!("{source}/{provider}")
    }
}

pub fn source_provider_counts(sessions: &[ListedSession]) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for listed in sessions {
        let label = source_provider_label(&listed.source, listed.session.provider);
        if let Some((_, count)) = counts.iter_mut().find(|(existing, _)| existing == &label) {
            *count += 1;
        } else {
            counts.push((label, 1));
        }
    }
    counts
}

fn compare_listed_sessions(a: &ListedSession, b: &ListedSession) -> std::cmp::Ordering {
    b.session
        .started_at
        .cmp(&a.session.started_at)
        .then_with(|| a.session.id.0.cmp(&b.session.id.0))
        .then_with(|| a.session.identity_key().cmp(&b.session.identity_key()))
}

fn listed_session_is_after_cursor(listed: &ListedSession, cursor: &ListCursor) -> bool {
    if listed.session.started_at != cursor.started_at {
        return listed.session.started_at < cursor.started_at;
    }
    if listed.session.id.0 != cursor.session_id {
        return listed.session.id.0 > cursor.session_id;
    }

    if cursor.session_key.is_empty() {
        return false;
    }
    listed.session.identity_key().as_str() > cursor.session_key.as_str()
}

fn session_matches(
    session: &Session,
    filters: &SearchFilters,
    project_needle: Option<&str>,
) -> bool {
    if let Some(want) = filters.provider {
        if session.provider != want {
            return false;
        }
    }
    if let Some(since) = filters.since {
        if session.started_at < since {
            return false;
        }
    }
    if let Some(until) = filters.until {
        if session.started_at > until {
            return false;
        }
    }
    if let Some(needle) = project_needle {
        let project = session
            .project_name
            .as_deref()
            .map(str::to_lowercase)
            .unwrap_or_default();
        if !project.contains(needle) {
            return false;
        }
    }
    true
}

fn session_has_matching_message(
    providers: &[Box<dyn HistoryProvider>],
    session: &Session,
    filters: &SearchFilters,
) -> Result<bool, String> {
    let messages = provider::load_messages_for_session(session, providers)
        .map_err(|error| error.to_string())?;
    Ok(messages.iter().any(|message| {
        if let Some(role) = filters.role {
            if message.role != role {
                return false;
            }
        }
        if filters.has_tool_call
            && !message
                .content
                .iter()
                .any(|block| matches!(block, ContentBlock::ToolUse(_)))
        {
            return false;
        }
        true
    }))
}

fn metadata_filter_matches_source(
    session: &Session,
    source: &str,
    metadata_keys: Option<&HashSet<String>>,
) -> bool {
    let Some(keys) = metadata_keys else {
        return true;
    };
    keys.contains(&qualified_session_metadata_key(session, source))
}

fn qualified_session_metadata_key(session: &Session, source: &str) -> String {
    let raw = session.session_ref().to_string();
    if source == LOCAL_SOURCE {
        raw
    } else {
        format!("{source}:{raw}")
    }
}

fn source_for_session<'a>(
    source_by_session: &'a std::collections::HashMap<String, String>,
    session: &Session,
) -> &'a str {
    source_by_session
        .get(session.identity_key().as_str())
        .map_or(LOCAL_SOURCE, String::as_str)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use chrono::TimeZone as _;

    use super::*;
    use crate::model::{Message, Role, SessionId};
    use crate::provider::ProviderError;

    struct FailingProvider {
        base_dirs: Vec<PathBuf>,
    }

    impl HistoryProvider for FailingProvider {
        fn provider(&self) -> Provider {
            Provider::ClaudeCode
        }

        fn base_dirs(&self) -> &[PathBuf] {
            &self.base_dirs
        }

        fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
            Ok(Vec::new())
        }

        fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
            Err(ProviderError::Parse {
                path: session.source_path.clone(),
                reason: "bad fixture".to_string(),
            })
        }
    }

    fn test_session() -> Session {
        Session {
            id: SessionId("bad-session".to_string()),
            provider: Provider::ClaudeCode,
            project_path: None,
            project_name: Some("broken".to_string()),
            git_branch: None,
            started_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            ended_at: None,
            summary: None,
            model: None,
            token_usage: None,
            message_count: 1,
            source_path: PathBuf::from("/tmp/bad-session.jsonl"),
        }
    }

    #[test]
    fn message_filter_records_warning_when_session_load_fails() {
        let session = test_session();
        let source_by_session = HashMap::from([(session.identity_key(), LOCAL_SOURCE.to_string())]);
        let discovery = FederatedDiscovery {
            sessions: vec![session],
            source_by_session,
            failures: Vec::new(),
        };
        let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(FailingProvider {
            base_dirs: Vec::new(),
        })];
        let filters = SearchFilters {
            role: Some(Role::User),
            ..SearchFilters::default()
        };

        let page = list_sessions_page(
            &providers,
            discovery,
            ListSessionsRequest {
                limit: 10,
                cursor: None,
                filters: &filters,
                metadata_keys: None,
            },
        )
        .unwrap();

        assert!(page.sessions.is_empty());
        assert_eq!(page.warnings.len(), 1);
        assert!(page.warnings[0]
            .warning_line()
            .contains("claude-code/bad-session"));
        assert!(page.warnings[0].warning_line().contains("bad fixture"));
    }
}
