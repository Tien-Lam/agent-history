use aghist::{federated, provider, query_scope};

pub(crate) fn federated_discovery_for_commands(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
) -> federated::FederatedDiscovery {
    let result = scope.discover_federated(providers);
    for failure in &result.failures {
        eprintln!("warning: source '{}': {}", failure.source, failure.message);
    }
    result
}

pub(crate) use aghist::session_resolver::{
    qualified_citation_ref, qualified_session_ref, source_for_session,
};
