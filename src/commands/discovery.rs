use std::collections::HashMap;

use aghist::model::Session;
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

pub(crate) fn source_for_session<'a>(
    source_by_session: &'a HashMap<String, String>,
    session: &Session,
) -> &'a str {
    source_by_session
        .get(session.identity_key().as_str())
        .map_or(federated::LOCAL_SOURCE, String::as_str)
}

pub(crate) fn qualified_session_ref(
    source_by_session: &HashMap<String, String>,
    session: &Session,
) -> String {
    let session_ref = session.session_ref().to_string();
    let source = source_for_session(source_by_session, session);
    if source == federated::LOCAL_SOURCE {
        session_ref
    } else {
        format!("{source}:{session_ref}")
    }
}
