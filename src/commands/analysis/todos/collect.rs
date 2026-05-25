use std::collections::HashSet;

use aghist::session_warnings::SessionLoadWarning;
use aghist::todos::{self, TodoCandidate, TodoKind};
use aghist::{provider, query_scope};

use crate::cli::FilterArgs;
use crate::commands::discovery::{federated_discovery_for_commands, source_for_session};
use crate::commands::filtering::{
    message_matches, metadata_filter_matches_source, session_matches,
};

use super::TodoRow;

pub(super) fn collect_federated_todo_candidates(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
    kinds: &[TodoKind],
) -> Vec<TodoRow> {
    let project_needle = filters.project_needle();

    let discovery = federated_discovery_for_commands(providers, scope);
    let mut candidates = Vec::new();

    for session in discovery.sessions {
        let source = source_for_session(&discovery.source_by_session, &session).to_string();
        if !session_matches(&session, filters, project_needle.as_deref()) {
            continue;
        }
        if !metadata_filter_matches_source(&session, &source, metadata_keys) {
            continue;
        }
        let messages = match provider::load_messages_for_session(&session, providers) {
            Ok(messages) => messages,
            Err(error) => {
                eprintln!(
                    "{}",
                    SessionLoadWarning::new(&source, &session, error).warning_line()
                );
                continue;
            }
        };
        for candidate in
            todos::extract_from_messages(session.provider, &session.id, &messages, kinds)
        {
            if !candidate_matches_filters(&candidate, &messages, filters) {
                continue;
            }
            candidates.push(TodoRow {
                candidate,
                source: source.clone(),
                project: session.project_name.clone(),
                started_at: session.started_at,
            });
        }
    }

    candidates
}

fn candidate_matches_filters(
    candidate: &TodoCandidate,
    messages: &[aghist::model::Message],
    filters: &FilterArgs,
) -> bool {
    if filters.role.is_some() || filters.has_tool_call {
        let turn_idx = (candidate.citation.turn as usize).saturating_sub(1);
        let Some(message) = messages.get(turn_idx) else {
            return false;
        };
        if !message_matches(message, filters) {
            return false;
        }
    }
    if let Some(since) = filters.since {
        if candidate.timestamp < since {
            return false;
        }
    }
    if let Some(until) = filters.until {
        if candidate.timestamp > until {
            return false;
        }
    }
    true
}
