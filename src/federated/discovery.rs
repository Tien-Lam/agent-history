use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::Path;

use crate::config::RemoteSource;
use crate::federated::{FederatedDiscovery, SourceFailure, LOCAL_SOURCE};
use crate::model::{Provider, Session};
use crate::provider::{self, HistoryProvider};

type DiscoveryOutcome = (String, Vec<Session>, Option<SourceFailure>);

/// Construct provider instances rooted at a remote source's `data_dir`.
///
/// The remote rsync target may be either a full home directory (so providers
/// live under their conventional subpaths — `.claude`, `.codex/sessions`,
/// etc.) or the exact provider directory (so `data_dir` itself is the
/// provider's history dir). We cover both interpretations by passing both
/// candidate base dirs to each provider; providers that don't recognise
/// either return zero sessions and contribute nothing.
pub fn providers_rooted_at(root: &Path) -> Vec<Box<dyn HistoryProvider>> {
    let mut providers: Vec<Box<dyn HistoryProvider>> = Vec::new();

    for &kind in Provider::all() {
        let dirs = provider::registry::remote_candidate_dirs(kind, root);
        if dirs.iter().any(|d| d.exists()) {
            providers.push(provider::registry::provider_from_dirs(kind, dirs));
        }
    }

    providers
}

fn discover_remote_sources_outcomes(
    sources: &[RemoteSource],
    cache_root: &Path,
) -> Vec<DiscoveryOutcome> {
    std::thread::scope(|scope| {
        let remote_handles: Vec<_> = sources
            .iter()
            .map(|src| scope.spawn(move || discover_remote(src, cache_root)))
            .collect();

        let mut out = Vec::with_capacity(remote_handles.len());
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
    })
}

fn discover_local(local_providers: &[Box<dyn HistoryProvider>]) -> DiscoveryOutcome {
    let mut sessions = Vec::new();
    for p in local_providers {
        if let Ok(found) = p.discover_sessions() {
            sessions.extend(found);
        }
    }
    (LOCAL_SOURCE.to_string(), sessions, None)
}

fn discover_remote(src: &RemoteSource, cache_root: &Path) -> DiscoveryOutcome {
    if let Err(message) = src.validate() {
        return (
            src.name.clone(),
            Vec::new(),
            Some(SourceFailure {
                source: src.name.clone(),
                message,
            }),
        );
    }
    let data_dir = src.data_dir(cache_root);
    if !data_dir.exists() {
        return (
            src.name.clone(),
            Vec::new(),
            Some(SourceFailure {
                source: src.name.clone(),
                message: format!(
                    "cache missing at {} — run `aghist sources pull {}` first",
                    data_dir.display(),
                    src.name
                ),
            }),
        );
    }
    let providers = providers_rooted_at(&data_dir);
    let mut sessions = Vec::new();
    let mut first_err: Option<String> = None;
    for p in &providers {
        match p.discover_sessions() {
            Ok(found) => sessions.extend(found),
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e.to_string());
                }
            }
        }
    }
    let failure = first_err.map(|message| SourceFailure {
        source: src.name.clone(),
        message,
    });
    (src.name.clone(), sessions, failure)
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
    merge_discovery_outcomes(discover_remote_sources_outcomes(sources, cache_root))
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
            .map(|src| scope.spawn(move || discover_remote(src, cache_root)))
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
