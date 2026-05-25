use std::collections::HashSet;

use aghist::todos::{self, TodoCandidate, TodoKind};
use aghist::{provider, query_scope};

use crate::cli::FilterArgs;
use crate::commands::filtering::{
    collect_filtered_federated_sessions, load_messages_or_warn, PreparedFilters,
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

    let filtered = collect_filtered_federated_sessions(providers, scope, filters, metadata_keys);
    let mut candidates = Vec::new();

    for filtered_session in filtered.sessions {
        let source = filtered_session.source;
        let session = filtered_session.session;
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
