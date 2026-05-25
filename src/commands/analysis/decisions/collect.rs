use std::collections::HashSet;

use aghist::{provider, query_scope};

use crate::cli::FilterArgs;
use crate::commands::filtering::{
    collect_filtered_federated_sessions, load_messages_or_warn, PreparedFilters,
};

use super::DecisionRow;

#[derive(Clone, Copy)]
pub(super) struct DecisionCollectRequest<'a> {
    pub(super) filters: &'a FilterArgs,
    pub(super) session_needle: Option<&'a str>,
    pub(super) source_needle: Option<&'a str>,
    pub(super) metadata_keys: Option<&'a HashSet<String>>,
    pub(super) threshold: f32,
}

/// Run the heuristic across matching local + remote-source sessions.
pub(super) fn collect_federated_decision_rows(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    request: DecisionCollectRequest<'_>,
) -> Vec<DecisionRow> {
    let DecisionCollectRequest {
        filters,
        session_needle,
        source_needle,
        metadata_keys,
        threshold,
    } = request;
    let filter_args = filters;
    let prepared_filters = PreparedFilters::from_args(filter_args);
    let filtered =
        collect_filtered_federated_sessions(providers, scope, filter_args, metadata_keys);
    let mut rows: Vec<DecisionRow> = Vec::new();
    for filtered_session in filtered.sessions {
        let source = filtered_session.source;
        let session = filtered_session.session;
        if source_needle.is_some_and(|want| want != source) {
            continue;
        }
        if let Some(needle) = session_needle {
            if !session.id.0.starts_with(needle) {
                continue;
            }
        }
        let Some(messages) = load_messages_or_warn(providers, &source, &session) else {
            continue;
        };
        let scored: Vec<_> = messages
            .iter()
            .enumerate()
            .filter(|(_, message)| prepared_filters.matches_message(message))
            .collect();
        for (idx, msg) in scored {
            let turn = u32::try_from(idx + 1).unwrap_or(u32::MAX);
            let candidates = aghist::decisions::extract_from_message(msg, turn, threshold);
            for candidate in candidates {
                let Some(citation) = aghist::model::CitationRef::new(
                    session.provider,
                    session.id.clone(),
                    candidate.turn,
                ) else {
                    continue;
                };
                rows.push(DecisionRow {
                    citation,
                    candidate,
                    source: source.clone(),
                    project: session.project_name.clone(),
                    started_at: session.started_at,
                });
            }
        }
    }
    rows
}
