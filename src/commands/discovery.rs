use aghist::{config, federated, provider};

pub(crate) fn federated_discovery_for_commands(
    providers: &[Box<dyn provider::HistoryProvider>],
) -> federated::FederatedDiscovery {
    let config = match config::Config::resolved_path() {
        Some(path) => config::Config::load_from(&path),
        None => config::Config::default(),
    };
    let enabled = config.enabled_providers();
    let mut result = if let Some(cache_root) = config::sources_cache_root() {
        federated::discover_federated(providers, &config.sources, &cache_root)
    } else {
        federated::discover_federated(providers, &[], std::path::Path::new(""))
    };
    result.retain_providers(&enabled);
    for failure in &result.failures {
        eprintln!("warning: source '{}': {}", failure.source, failure.message);
    }
    result
}

pub(crate) use aghist::session_resolver::{
    qualified_citation_ref, qualified_session_ref, source_for_session,
};
