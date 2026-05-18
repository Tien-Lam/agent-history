use std::collections::{HashMap, HashSet};

use aghist::model::{Message, Session};
use aghist::session_warnings::SessionLoadWarning;
use aghist::{provider, query_scope};

use super::super::discovery::{federated_discovery_for_commands, source_for_session};
use super::super::filtering::{metadata_filter_matches_source, session_matches};
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
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    project_needle: Option<&str>,
    metadata_keys: Option<&HashSet<String>>,
) -> Vec<Session> {
    let discovery = federated_discovery_for_commands(providers, scope);
    let source_by_session = discovery.source_by_session;
    discovery
        .sessions
        .into_iter()
        .filter(|session| session_matches(session, filters, project_needle))
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
    project_needle: Option<&str>,
    metadata_keys: Option<&HashSet<String>>,
    include_session: impl Fn(&Session) -> bool,
) -> FederatedSessionBundles {
    let discovery = federated_discovery_for_commands(providers, scope);
    let source_by_session = discovery.source_by_session;
    let mut bundles = Vec::new();
    for session in discovery.sessions {
        if !session_matches(&session, filters, project_needle) {
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
        let messages = match provider::load_messages_for_session(&session, providers) {
            Ok(messages) => messages,
            Err(error) => {
                let source = source_for_session(&source_by_session, &session);
                eprintln!(
                    "{}",
                    SessionLoadWarning::new(source, &session, error).warning_line()
                );
                continue;
            }
        };
        bundles.push((session, messages));
    }
    FederatedSessionBundles {
        bundles,
        source_by_session,
    }
}
