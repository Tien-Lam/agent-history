use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Utc;

use super::{LookupSource, ResolutionError, SelectorShape, SessionResolver};
use crate::federated::LOCAL_SOURCE;
use crate::model::{Provider, Session, SessionId};

fn session(provider: Provider, id: &str, path: &str) -> Session {
    Session {
        id: SessionId(id.to_string()),
        provider,
        project_path: None,
        project_name: None,
        git_branch: None,
        started_at: Utc::now(),
        ended_at: None,
        summary: None,
        model: None,
        token_usage: None,
        message_count: 1,
        source_path: PathBuf::from(path),
    }
}

fn source_map(sessions: &[Session], remote: &str) -> HashMap<String, String> {
    sessions
        .iter()
        .map(|s| {
            let source = if s.source_path.to_string_lossy().contains("remote") {
                remote
            } else {
                LOCAL_SOURCE
            };
            (s.identity_key(), source.to_string())
        })
        .collect()
}

#[test]
fn resolves_source_qualified_session_ref() {
    let sessions = vec![
        session(Provider::ClaudeCode, "shared", "/local/shared.jsonl"),
        session(Provider::ClaudeCode, "shared", "/remote/shared.jsonl"),
    ];
    let sources = source_map(&sessions, "laptop");
    let resolver = SessionResolver::new(&sessions, &sources);

    let selected = resolver
        .resolve_session_selector(
            "laptop:claude-code/shared",
            SelectorShape::SessionRefOrIdPrefix,
        )
        .unwrap();

    assert_eq!(selected.source, "laptop");
    assert_eq!(selected.session_ref, "laptop:claude-code/shared");
}

#[test]
fn unqualified_duplicate_session_id_is_ambiguous() {
    let sessions = vec![
        session(Provider::ClaudeCode, "shared", "/local/shared.jsonl"),
        session(Provider::ClaudeCode, "shared", "/remote/shared.jsonl"),
    ];
    let sources = source_map(&sessions, "laptop");
    let resolver = SessionResolver::new(&sessions, &sources);

    let err = resolver
        .resolve_session_selector("shared", SelectorShape::SessionRefOrIdPrefix)
        .unwrap_err();

    match err {
        ResolutionError::Ambiguous { candidates, .. } => {
            assert!(candidates.contains(&"claude-code/shared".to_string()));
            assert!(candidates.contains(&"laptop:claude-code/shared".to_string()));
        }
        other => panic!("expected ambiguous error, got {other:?}"),
    }
}

#[test]
fn explicit_local_source_is_valid_for_lookup() {
    let sessions = vec![session(
        Provider::ClaudeCode,
        "only-local",
        "/local/only-local.jsonl",
    )];
    let sources = source_map(&sessions, "laptop");
    let resolver = SessionResolver::new(&sessions, &sources);

    let selected = resolver
        .find_exact(Provider::ClaudeCode, "only-local", LookupSource::Local)
        .unwrap();

    assert_eq!(selected.source, LOCAL_SOURCE);
    assert_eq!(selected.session_ref, "claude-code/only-local");
}

#[test]
fn exact_lookup_can_target_any_source_explicitly() {
    let sessions = vec![
        session(Provider::ClaudeCode, "shared", "/local/shared.jsonl"),
        session(Provider::ClaudeCode, "shared", "/remote/shared.jsonl"),
    ];
    let sources = source_map(&sessions, "laptop");
    let resolver = SessionResolver::new(&sessions, &sources);

    let err = resolver
        .find_exact(Provider::ClaudeCode, "shared", LookupSource::Any)
        .unwrap_err();
    assert!(matches!(err, ResolutionError::Ambiguous { .. }));

    let selected = resolver
        .find_exact(
            Provider::ClaudeCode,
            "shared",
            LookupSource::Named("laptop"),
        )
        .unwrap();
    assert_eq!(selected.source, "laptop");
    assert_eq!(selected.session_ref, "laptop:claude-code/shared");
}

#[test]
fn citation_resolution_preserves_remote_source_in_ref() {
    let sessions = vec![session(
        Provider::ClaudeCode,
        "remote-session",
        "/remote/session.jsonl",
    )];
    let sources = source_map(&sessions, "laptop");
    let resolver = SessionResolver::new(&sessions, &sources);

    let selected = resolver
        .resolve_citation_selector("laptop:claude-code/remote-session#3")
        .unwrap();

    assert_eq!(selected.source, "laptop");
    assert_eq!(selected.citation_ref, "laptop:claude-code/remote-session#3");
}
