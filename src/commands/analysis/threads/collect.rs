use std::collections::HashSet;

use aghist::{provider, query_scope};

use crate::cli::FilterArgs;
use crate::commands::discovery::{federated_discovery_for_commands, source_for_session};
use crate::commands::filtering::{metadata_filter_matches_source, PreparedFilters};

pub(super) fn collect_federated_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
) -> aghist::federated::FederatedDiscovery {
    let filters = PreparedFilters::from_args(filters);

    let mut discovery = federated_discovery_for_commands(providers, scope);
    let source_by_session = &discovery.source_by_session;
    discovery.sessions.retain(|session| {
        filters.matches_session(session)
            && metadata_filter_matches_source(
                session,
                source_for_session(source_by_session, session),
                metadata_keys,
            )
    });
    discovery
}
