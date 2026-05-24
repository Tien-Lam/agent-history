use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::Path;

use crate::config::RemoteSource;
use crate::federated::{FederatedDiscovery, SourceFailure, LOCAL_SOURCE};
use crate::model::Session;
use crate::provider::HistoryProvider;

mod remote;

pub use remote::providers_rooted_at;

pub(super) type DiscoveryOutcome = (String, Vec<Session>, Option<SourceFailure>);

fn discover_local(local_providers: &[Box<dyn HistoryProvider>]) -> DiscoveryOutcome {
    discover_provider_sessions(LOCAL_SOURCE, local_providers)
}

pub(super) fn discover_provider_sessions(
    source: &str,
    providers: &[Box<dyn HistoryProvider>],
) -> DiscoveryOutcome {
    let mut sessions = Vec::new();
    let mut errors = Vec::new();
    for p in providers {
        match p.discover_sessions() {
            Ok(found) => sessions.extend(found),
            Err(error) => errors.push(format!(
                "provider '{}' discovery failed: {error}",
                p.provider().slug()
            )),
        }
    }
    let failure = (!errors.is_empty()).then(|| SourceFailure {
        source: source.to_string(),
        message: errors.join("; "),
    });
    (source.to_string(), sessions, failure)
}

fn merge_discovery_outcomes(outcomes: Vec<DiscoveryOutcome>) -> FederatedDiscovery {
    // Outcomes are ordered local-first. Deduplicate only exact identities
    // rather than raw provider session ids: different providers or remote
    // sources can legitimately reuse the same id.
    let mut sessions = Vec::new();
    let mut source_by_session: HashMap<String, String> = HashMap::new();
    let mut failures = Vec::new();
    for (tag, batch, failure) in outcomes {
        for s in batch {
            if let Entry::Vacant(v) = source_by_session.entry(s.identity_key()) {
                v.insert(tag.clone());
                sessions.push(s);
            }
        }
        if let Some(f) = failure {
            failures.push(f);
        }
    }

    FederatedDiscovery {
        sessions,
        source_by_session,
        failures,
    }
}

/// Discover only registered remote sources, without scanning local provider
/// directories. This is useful for commands that already performed local
/// discovery and only need to add remote cache contents.
pub fn discover_remote_sources(sources: &[RemoteSource], cache_root: &Path) -> FederatedDiscovery {
    merge_discovery_outcomes(remote::discover_remote_sources_outcomes(
        sources, cache_root,
    ))
}

/// Discover sessions concurrently from local providers + every registered
/// remote source whose cache exists. Returns sessions in the order:
/// `[local..., source_1..., source_2..., ...]` (sources in the order given).
///
/// `cache_root` is the parent of `<name>/data/` directories — typically
/// [`crate::config::sources_cache_root`]. Sources whose `data_dir` does not
/// exist yet (never pulled) are recorded as failures with a hint, but do not
/// stop discovery for the rest.
pub fn discover_federated(
    local_providers: &[Box<dyn HistoryProvider>],
    sources: &[RemoteSource],
    cache_root: &Path,
) -> FederatedDiscovery {
    // Discovery is IO-bound; fan out across local plus each remote source.
    let outcomes: Vec<DiscoveryOutcome> = std::thread::scope(|scope| {
        // Spawn one thread per source (local counts as one). Local goes first
        // so its handle is joined first and its sessions land at the front of
        // the merged vec, keeping deterministic ordering for callers that
        // don't sort.
        let local_handle = scope.spawn(|| discover_local(local_providers));
        let remote_handles: Vec<_> = sources
            .iter()
            .map(|src| scope.spawn(move || remote::discover_remote(src, cache_root)))
            .collect();

        let mut out = Vec::with_capacity(remote_handles.len() + 1);
        // join() on a scoped handle only fails if the thread panicked; treat
        // a panic as a source failure rather than aborting the parent.
        match local_handle.join() {
            Ok(o) => out.push(o),
            Err(_) => out.push((
                LOCAL_SOURCE.to_string(),
                Vec::new(),
                Some(SourceFailure {
                    source: LOCAL_SOURCE.to_string(),
                    message: "local discovery thread panicked".to_string(),
                }),
            )),
        }
        for (src, h) in sources.iter().zip(remote_handles) {
            match h.join() {
                Ok(o) => out.push(o),
                Err(_) => out.push((
                    src.name.clone(),
                    Vec::new(),
                    Some(SourceFailure {
                        source: src.name.clone(),
                        message: "remote discovery thread panicked".to_string(),
                    }),
                )),
            }
        }
        out
    });

    merge_discovery_outcomes(outcomes)
}
