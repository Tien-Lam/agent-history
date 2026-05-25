use std::collections::{HashMap, HashSet};

use aghist::model::{Message, Session};
use aghist::{provider, query_scope};

use super::super::filtering::{collect_filtered_federated_sessions, load_messages_or_warn};
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
    collect_filtered_federated_sessions(providers, scope, filters, metadata_keys)
        .sessions
        .into_iter()
        .map(|filtered| filtered.session)
        .collect()
}

pub(super) fn collect_federated_message_bundles(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
    include_session: impl Fn(&Session) -> bool,
) -> FederatedSessionBundles {
    let filtered = collect_filtered_federated_sessions(providers, scope, filters, metadata_keys);
    let mut bundles = Vec::new();
    for filtered_session in filtered.sessions {
        let source = filtered_session.source;
        let session = filtered_session.session;
        if !include_session(&session) {
            continue;
        }
        let Some(messages) = load_messages_or_warn(providers, &source, &session) else {
            continue;
        };
        bundles.push((session, messages));
    }
    FederatedSessionBundles {
        bundles,
        source_by_session: filtered.source_by_session,
    }
}
