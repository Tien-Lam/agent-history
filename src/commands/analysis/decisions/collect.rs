use std::collections::HashSet;

use aghist::{provider, query_scope};

use crate::cli::FilterArgs;
use crate::commands::discovery::{federated_discovery_for_commands, source_for_session};
use crate::commands::filtering::{
    message_matches, metadata_filter_matches_source, session_matches,
};

use super::DecisionRow;

#[derive(Clone, Copy)]
pub(super) struct DecisionCollectRequest<'a> {
    pub(super) filters: &'a FilterArgs,
    pub(super) project_needle: Option<&'a str>,
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
        project_needle,
        session_needle,
        source_needle,
        metadata_keys,
        threshold,
    } = request;
    let discovery = federated_discovery_for_commands(providers, scope);
    let mut rows: Vec<DecisionRow> = Vec::new();
    for session in discovery.sessions {
        let source = source_for_session(&discovery.source_by_session, &session).to_string();
        if source_needle.is_some_and(|want| want != source) {
            continue;
        }
        if !session_matches(&session, filters, project_needle) {
            continue;
        }
        if !metadata_filter_matches_source(&session, &source, metadata_keys) {
            continue;
        }
        if let Some(needle) = session_needle {
            if !session.id.0.starts_with(needle) {
                continue;
            }
        }
        let Ok(messages) = provider::load_messages_for_session(&session, providers) else {
            continue;
        };
        let scored: Vec<_> = messages
            .iter()
            .enumerate()
            .filter(|(_, message)| message_matches(message, filters))
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
