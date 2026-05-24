use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::*;
use crate::config::{RemoteSource, Transport};
use crate::model::{Message, Provider, Session};
use crate::provider::{claude_code::ClaudeCodeProvider, HistoryProvider, ProviderError};

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

struct FailingProvider;

impl HistoryProvider for FailingProvider {
    fn provider(&self) -> Provider {
        Provider::ClaudeCode
    }

    fn base_dirs(&self) -> &[std::path::PathBuf] {
        &[]
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        Err(ProviderError::Parse {
            path: "bad-provider-state".into(),
            reason: "fixture discovery failed".to_string(),
        })
    }

    fn load_messages(&self, _session: &Session) -> Result<Vec<Message>, ProviderError> {
        Ok(Vec::new())
    }
}

#[test]
fn local_only_when_no_sources_registered() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    write_claude_fixture(home, "local-1");

    let local = ClaudeCodeProvider::new(vec![home.join(".claude")]);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(local)];

    let cache = tmp.path().join("cache");
    let result = discover_federated(&providers, &[], &cache);
    assert_eq!(result.sessions.len(), 1);
    assert_eq!(result.source_of_session(&result.sessions[0]), LOCAL_SOURCE);
    assert!(result.failures.is_empty());
}

#[test]
fn local_provider_discovery_errors_are_reported() {
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(FailingProvider)];
    let cache = tempfile::tempdir().unwrap();

    let result = discover_federated(&providers, &[], cache.path());

    assert!(result.sessions.is_empty());
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].source, LOCAL_SOURCE);
    assert!(
        result.failures[0]
            .message
            .contains("provider 'claude-code' discovery failed"),
        "{:?}",
        result.failures
    );
}

#[test]
fn remote_source_sessions_are_tagged_with_source_name() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    write_claude_fixture(&home, "local-1");

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
fn remote_only_discovery_skips_local_source_tag() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().join("cache");
    let remote_data = cache.join("laptop").join("data");
    std::fs::create_dir_all(&remote_data).unwrap();
    write_claude_fixture(&remote_data, "remote-only");

    let sources = vec![RemoteSource {
        name: "laptop".to_string(),
        host: "laptop.local".to_string(),
        path: "/home/x".to_string(),
        transport: Transport::Ssh,
    }];

    let result = discover_remote_sources(&sources, &cache);
    assert_eq!(result.sessions.len(), 1);
    assert_eq!(result.source_of_session(&result.sessions[0]), "laptop");
    assert!(result.failures.is_empty());
}

#[test]
fn retain_providers_prunes_sessions_and_source_map() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    write_claude_fixture(&home, "local-1");

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

    let mut result = discover_federated(&providers, &sources, &cache);
    result.retain_providers(&HashSet::new());
    assert!(result.sessions.is_empty());
    assert!(result.source_by_session.is_empty());
}

#[test]
fn raw_session_id_overlap_across_sources_is_preserved() {
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
    let sources: HashSet<&str> = shared.iter().map(|s| result.source_of_session(s)).collect();
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

#[test]
fn source_errors_use_stable_json_shape() {
    let failures = vec![SourceFailure {
        source: "desk".to_string(),
        message: "cache missing".to_string(),
    }];

    let errors = source_errors(&failures);
    assert_eq!(
        serde_json::to_value(&errors).unwrap(),
        serde_json::json!([{ "source": "desk", "error": "cache missing" }])
    );
    assert_eq!(
        failures[0].warning_line(),
        "warning: source 'desk': cache missing"
    );
}
