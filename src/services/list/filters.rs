use std::collections::HashSet;

use crate::model::{ContentBlock, Session};
use crate::provider::{self, HistoryProvider};
use crate::search::SearchFilters;
use crate::session_resolver::qualified_session_metadata_key;

pub(super) fn session_matches(
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

pub(super) fn session_has_matching_message(
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

pub(super) fn metadata_filter_matches_source(
    session: &Session,
    source: &str,
    metadata_keys: Option<&HashSet<String>>,
) -> bool {
    let Some(keys) = metadata_keys else {
        return true;
    };
    keys.contains(&qualified_session_metadata_key(session, source))
}
