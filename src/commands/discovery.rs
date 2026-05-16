use aghist::{config, federated, provider, query_scope};

pub(crate) fn federated_discovery_for_commands(
    providers: &[Box<dyn provider::HistoryProvider>],
) -> federated::FederatedDiscovery {
    let config = match config::Config::resolved_path() {
        Some(path) => config::Config::load_from(&path),
        None => config::Config::default(),
    };
    let result = query_scope::QueryScope::enabled(&config).discover_federated(providers);
    for failure in &result.failures {
        eprintln!("warning: source '{}': {}", failure.source, failure.message);
    }
    result
}

pub(crate) use aghist::session_resolver::{
    qualified_citation_ref, qualified_session_ref, source_for_session,
};
