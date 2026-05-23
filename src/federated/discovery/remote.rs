use std::path::Path;

use crate::config::RemoteSource;
use crate::federated::SourceFailure;
use crate::model::Provider;
use crate::provider::{self, HistoryProvider};

use super::DiscoveryOutcome;

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

pub(super) fn discover_remote_sources_outcomes(
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

pub(super) fn discover_remote(src: &RemoteSource, cache_root: &Path) -> DiscoveryOutcome {
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
