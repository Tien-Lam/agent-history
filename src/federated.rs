//! Federated session discovery across local providers and remote source caches.
//!
//! Local provider directories (e.g. `~/.claude`, `~/.codex`) and remote source
//! caches under `<sources_cache>/<name>/data/` are scanned together. Each
//! scan runs in its own thread (concurrent fanout); a failing source is
//! recorded but does not abort the others (partial-failure tolerant).
//!
//! The result tags every session with the source it came from — `"local"` for
//! the host running aghist, or the registered source name for remote mirrors —
//! so search results can surface a `source` marker. Callers consult
//! [`FederatedDiscovery::source_of_session`] to map a session back to its
//! source.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::Path;

use crate::config::RemoteSource;
use crate::model::{Provider, Session};
use crate::provider::{self, HistoryProvider};

/// Source tag for sessions discovered from local provider dirs.
pub const LOCAL_SOURCE: &str = "local";

/// Outcome of [`discover_federated`]. `sessions` are concatenated from local +
/// every reachable remote source. `failures` records sources whose discovery
/// raised an error or whose cache directory was missing — surfaced to the
/// caller for diagnostics, never fatal.
pub struct FederatedDiscovery {
    pub sessions: Vec<Session>,
    /// Maps `Session::identity_key()` to the source tag (`"local"` or a
    /// registered source name). Sessions with no entry default to `"local"` —
    /// useful for code paths that did not go through federated discovery.
    pub source_by_session: HashMap<String, String>,
    pub failures: Vec<SourceFailure>,
}

#[derive(Debug, Clone)]
pub struct SourceFailure {
    pub source: String,
    pub message: String,
}

type DiscoveryOutcome = (String, Vec<Session>, Option<SourceFailure>);

impl FederatedDiscovery {
    /// Returns the source tag for a session, defaulting to `"local"` when the
    /// session was never seen by federated discovery.
    pub fn source_of_session(&self, session: &Session) -> &str {
        self.source_by_session
            .get(session.identity_key().as_str())
            .map_or(LOCAL_SOURCE, String::as_str)
    }
}

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
        // the merged vec — keeps deterministic ordering for callers that don't
        // sort.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Transport;
    use crate::provider::claude_code::ClaudeCodeProvider;

    fn write_claude_fixture(home: &Path, session_id: &str) {
        let projects = home.join(".claude").join("projects").join("proj");
        std::fs::create_dir_all(&projects).unwrap();
        let session_file = projects.join(format!("{session_id}.jsonl"));
        let line = format!(
            r#"{{"type":"user","uuid":"u1","timestamp":"2025-01-01T00:00:00Z","sessionId":"{session_id}","cwd":"/p","message":{{"role":"user","content":"hello world"}}}}"#
        );
        std::fs::write(&session_file, format!("{line}\n")).unwrap();
        let history = home.join(".claude").join("history.jsonl");
        let entry = format!(
            r#"{{"display":"d","timestamp":1735689600000,"project":"proj","sessionId":"{session_id}"}}"#
        );
        std::fs::write(&history, format!("{entry}\n")).unwrap();
    }

    #[test]
    fn local_only_when_no_sources_registered() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        write_claude_fixture(home, "local-1");

        // Construct a local Claude provider rooted at `home/.claude` directly.
        let local = ClaudeCodeProvider::new(vec![home.join(".claude")]);
        let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(local)];

        let cache = tmp.path().join("cache");
        let result = discover_federated(&providers, &[], &cache);
        assert_eq!(result.sessions.len(), 1);
        assert_eq!(result.source_of_session(&result.sessions[0]), LOCAL_SOURCE);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn remote_source_sessions_are_tagged_with_source_name() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        write_claude_fixture(&home, "local-1");

        // Mirror a remote home dir under <cache>/laptop/data/, populated with
        // its own Claude tree — exercises the providers_rooted_at(home-style)
        // path.
        let cache = tmp.path().join("cache");
        let remote_data = cache.join("laptop").join("data");
        std::fs::create_dir_all(&remote_data).unwrap();
        write_claude_fixture(&remote_data, "remote-1");

        let local = ClaudeCodeProvider::new(vec![home.join(".claude")]);
        let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(local)];

        let sources = vec![RemoteSource {
            name: "laptop".to_string(),
            host: "laptop.local".to_string(),
            path: "/home/x".to_string(),
            transport: Transport::Ssh,
        }];

        let result = discover_federated(&providers, &sources, &cache);
        assert_eq!(result.sessions.len(), 2);

        let by_id: HashMap<_, _> = result
            .sessions
            .iter()
            .map(|s| (s.id.0.clone(), result.source_of_session(s).to_string()))
            .collect();
        assert_eq!(by_id.get("local-1").map(String::as_str), Some(LOCAL_SOURCE));
        assert_eq!(by_id.get("remote-1").map(String::as_str), Some("laptop"));
        assert!(result.failures.is_empty());
    }

    #[test]
    fn raw_session_id_overlap_across_sources_is_preserved() {
        // Same session_id present locally and on a remote mirror. Both rows
        // must survive because source path is part of the internal identity.
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        write_claude_fixture(&home, "shared-1");

        let cache = tmp.path().join("cache");
        let remote_data = cache.join("backup").join("data");
        std::fs::create_dir_all(&remote_data).unwrap();
        write_claude_fixture(&remote_data, "shared-1");

        let local = ClaudeCodeProvider::new(vec![home.join(".claude")]);
        let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(local)];

        let sources = vec![RemoteSource {
            name: "backup".to_string(),
            host: "backup.local".to_string(),
            path: "/home/x".to_string(),
            transport: Transport::Ssh,
        }];

        let result = discover_federated(&providers, &sources, &cache);
        let shared: Vec<_> = result
            .sessions
            .iter()
            .filter(|s| s.id.0 == "shared-1")
            .collect();
        assert_eq!(
            shared.len(),
            2,
            "duplicate raw session ids across sources must both survive; got {shared:?}"
        );
        let sources: std::collections::HashSet<&str> =
            shared.iter().map(|s| result.source_of_session(s)).collect();
        assert!(sources.contains(LOCAL_SOURCE), "{sources:?}");
        assert!(sources.contains("backup"), "{sources:?}");
        assert!(result.failures.is_empty());
    }

    #[test]
    fn missing_remote_cache_records_failure_but_does_not_abort() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        write_claude_fixture(&home, "local-1");

        // `desk` is registered but has never been pulled — its data_dir does
        // not exist. Discovery for `laptop` (which IS pulled) must still run.
        let cache = tmp.path().join("cache");
        let laptop_data = cache.join("laptop").join("data");
        std::fs::create_dir_all(&laptop_data).unwrap();
        write_claude_fixture(&laptop_data, "remote-laptop");

        let local = ClaudeCodeProvider::new(vec![home.join(".claude")]);
        let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(local)];

        let sources = vec![
            RemoteSource {
                name: "desk".to_string(),
                host: "desk.local".to_string(),
                path: "/home/x".to_string(),
                transport: Transport::Ssh,
            },
            RemoteSource {
                name: "laptop".to_string(),
                host: "laptop.local".to_string(),
                path: "/home/x".to_string(),
                transport: Transport::Ssh,
            },
        ];

        let result = discover_federated(&providers, &sources, &cache);
        let ids: Vec<&str> = result.sessions.iter().map(|s| s.id.0.as_str()).collect();
        assert!(ids.contains(&"local-1"), "missing local: {ids:?}");
        assert!(ids.contains(&"remote-laptop"), "missing laptop: {ids:?}");
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].source, "desk");
    }
}
