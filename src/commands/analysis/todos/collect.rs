use std::collections::HashSet;

use aghist::todos::{self, TodoCandidate, TodoKind};
use aghist::{provider, query_scope};

use crate::cli::FilterArgs;
use crate::commands::discovery::{federated_discovery_for_commands, source_for_session};
use crate::commands::filtering::{
    load_messages_or_warn, metadata_filter_matches_source, PreparedFilters,
};

use super::TodoRow;

pub(super) fn collect_federated_todo_candidates(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
    kinds: &[TodoKind],
) -> Vec<TodoRow> {
    let prepared_filters = PreparedFilters::from_args(filters);

    let discovery = federated_discovery_for_commands(providers, scope);
    let mut candidates = Vec::new();

    for session in discovery.sessions {
        let source = source_for_session(&discovery.source_by_session, &session).to_string();
        if !prepared_filters.matches_session(&session) {
            continue;
        }
        if !metadata_filter_matches_source(&session, &source, metadata_keys) {
            continue;
        }
        let Some(messages) = load_messages_or_warn(providers, &source, &session) else {
            continue;
        };
        for candidate in
            todos::extract_from_messages(session.provider, &session.id, &messages, kinds)
        {
            if !candidate_matches_filters(&candidate, &messages, filters, &prepared_filters) {
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
    prepared_filters: &PreparedFilters,
) -> bool {
    if filters.role.is_some() || filters.has_tool_call {
        let turn_idx = (candidate.citation.turn as usize).saturating_sub(1);
        let Some(message) = messages.get(turn_idx) else {
            return false;
        };
        if !prepared_filters.matches_message(message) {
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
