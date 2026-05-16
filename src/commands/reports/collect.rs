use std::collections::HashMap;

use aghist::model::{Message, Session};
use aghist::provider;

use super::super::discovery::federated_discovery_for_commands;
use super::super::filtering::session_matches;
use crate::cli::FilterArgs;

pub(super) type SessionBundle = (Session, Vec<Message>);

pub(super) struct FederatedSessionBundles {
    pub(super) bundles: Vec<SessionBundle>,
    pub(super) source_by_session: HashMap<String, String>,
}

pub(super) fn normalized_project_filter(filters: &FilterArgs) -> Option<String> {
    filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty())
}

pub(super) fn collect_federated_filtered_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    project_needle: Option<&str>,
) -> Vec<Session> {
    federated_discovery_for_commands(providers)
        .sessions
        .into_iter()
        .filter(|session| session_matches(session, filters, project_needle))
        .collect()
}

pub(super) fn collect_federated_message_bundles(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    project_needle: Option<&str>,
    include_session: impl Fn(&Session) -> bool,
) -> FederatedSessionBundles {
    let discovery = federated_discovery_for_commands(providers);
    let source_by_session = discovery.source_by_session;
    let mut bundles = Vec::new();
    for session in discovery.sessions {
        if !session_matches(&session, filters, project_needle) {
            continue;
        }
        if !include_session(&session) {
            continue;
        }
        let Ok(messages) = provider::load_messages_for_session(&session, providers) else {
            continue;
        };
        bundles.push((session, messages));
    }
    FederatedSessionBundles {
        bundles,
        source_by_session,
    }
}
