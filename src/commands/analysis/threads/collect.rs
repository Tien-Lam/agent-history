use std::collections::HashSet;

use aghist::{provider, query_scope};

use crate::cli::FilterArgs;
use crate::commands::filtering::collect_filtered_federated_sessions;

pub(super) fn collect_federated_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
) -> aghist::federated::FederatedDiscovery {
    collect_filtered_federated_sessions(providers, scope, filters, metadata_keys).into_discovery()
}
