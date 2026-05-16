use std::collections::HashSet;

use aghist::provider;

use crate::cli::FilterArgs;
use crate::commands::discovery::federated_discovery_for_commands;
use crate::commands::filtering::{metadata_filter_matches, session_matches};

pub(super) fn collect_federated_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
) -> aghist::federated::FederatedDiscovery {
    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut discovery = federated_discovery_for_commands(providers);
    discovery.sessions.retain(|session| {
        session_matches(session, filters, project_needle.as_deref())
            && metadata_filter_matches(session, metadata_keys)
    });
    discovery
}
