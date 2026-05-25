use std::collections::{HashMap, HashSet};

use aghist::model::{Message, Session};
use aghist::{provider, query_scope};

use super::super::discovery::{federated_discovery_for_commands, source_for_session};
use super::super::filtering::{
    load_messages_or_warn, metadata_filter_matches_source, PreparedFilters,
};
use crate::cli::FilterArgs;

pub(super) type SessionBundle = (Session, Vec<Message>);

pub(super) struct FederatedSessionBundles {
    pub(super) bundles: Vec<SessionBundle>,
    pub(super) source_by_session: HashMap<String, String>,
}

pub(super) fn collect_federated_filtered_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
) -> Vec<Session> {
    let filters = PreparedFilters::from_args(filters);
    let discovery = federated_discovery_for_commands(providers, scope);
    let source_by_session = discovery.source_by_session;
    discovery
        .sessions
        .into_iter()
        .filter(|session| filters.matches_session(session))
        .filter(|session| {
            metadata_filter_matches_source(
                session,
                source_for_session(&source_by_session, session),
                metadata_keys,
            )
        })
        .collect()
}

pub(super) fn collect_federated_message_bundles(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
    include_session: impl Fn(&Session) -> bool,
) -> FederatedSessionBundles {
    let filters = PreparedFilters::from_args(filters);
    let discovery = federated_discovery_for_commands(providers, scope);
    let source_by_session = discovery.source_by_session;
    let mut bundles = Vec::new();
    for session in discovery.sessions {
        if !filters.matches_session(&session) {
            continue;
        }
        if !metadata_filter_matches_source(
            &session,
            source_for_session(&source_by_session, &session),
            metadata_keys,
        ) {
            continue;
        }
        if !include_session(&session) {
            continue;
        }
        let source = source_for_session(&source_by_session, &session);
        let Some(messages) = load_messages_or_warn(providers, source, &session) else {
            continue;
        };
        bundles.push((session, messages));
    }
    FederatedSessionBundles {
        bundles,
        source_by_session,
    }
}
