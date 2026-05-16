use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};

use super::*;
use crate::model::{Provider, SessionId};

fn mk_session(
    id: &str,
    provider: Provider,
    project: Option<&str>,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
) -> Session {
    Session {
        id: SessionId(id.to_string()),
        provider,
        project_path: project.map(PathBuf::from),
        project_name: project.map(str::to_string),
        git_branch: None,
        started_at,
        ended_at,
        summary: None,
        model: None,
        token_usage: None,
        message_count: 1,
        source_path: PathBuf::from(format!("/tmp/{id}")),
    }
}

fn ts(secs: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(secs, 0).expect("valid ts")
}

#[test]
fn close_in_time_same_project_groups_into_one_thread() {
    let sessions = vec![
        mk_session("a", Provider::ClaudeCode, Some("foo"), ts(0), Some(ts(60))),
        mk_session(
            "b",
            Provider::ClaudeCode,
            Some("foo"),
            ts(120),
            Some(ts(180)),
        ),
    ];
    let threads = cluster(&sessions, ClusterOptions::default());
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].session_count, 2);
    assert_eq!(threads[0].project.as_deref(), Some("foo"));
    assert_eq!(threads[0].started_at, ts(0));
    assert_eq!(threads[0].ended_at, ts(180));
}

#[test]
fn far_apart_same_project_splits_into_two_threads() {
    let one_day = 24 * 3600;
    let sessions = vec![
        mk_session("a", Provider::ClaudeCode, Some("foo"), ts(0), Some(ts(60))),
        mk_session(
            "b",
            Provider::ClaudeCode,
            Some("foo"),
            ts(one_day),
            Some(ts(one_day + 60)),
        ),
    ];
    let threads = cluster(&sessions, ClusterOptions::default());
    assert_eq!(threads.len(), 2);
    assert_eq!(threads[0].started_at, ts(one_day));
    assert_eq!(threads[1].started_at, ts(0));
}

#[test]
fn different_projects_never_merge() {
    let sessions = vec![
        mk_session("a", Provider::ClaudeCode, Some("foo"), ts(0), None),
        mk_session("b", Provider::ClaudeCode, Some("bar"), ts(60), None),
    ];
    let threads = cluster(&sessions, ClusterOptions::default());
    assert_eq!(threads.len(), 2);
    let projects: Vec<&str> = threads
        .iter()
        .map(|t| t.project.as_deref().unwrap())
        .collect();
    assert!(projects.contains(&"foo"));
    assert!(projects.contains(&"bar"));
}

#[test]
fn missing_project_clusters_under_unknown_bucket() {
    let sessions = vec![
        mk_session("a", Provider::ClaudeCode, None, ts(0), None),
        mk_session("b", Provider::ClaudeCode, None, ts(60), None),
    ];
    let threads = cluster(&sessions, ClusterOptions::default());
    assert_eq!(threads.len(), 1);
    assert!(threads[0].project.is_none());
    assert_eq!(threads[0].session_count, 2);
}

#[test]
fn min_sessions_filters_singletons() {
    let one_day = 24 * 3600;
    let sessions = vec![
        mk_session("a", Provider::ClaudeCode, Some("foo"), ts(0), None),
        mk_session("b", Provider::ClaudeCode, Some("foo"), ts(one_day), None),
    ];
    let opts = ClusterOptions {
        min_sessions: 2,
        ..ClusterOptions::default()
    };
    let threads = cluster(&sessions, opts);
    assert!(threads.is_empty());
}

#[test]
fn session_refs_use_kebab_provider_slug() {
    let s = mk_session("abc-123", Provider::ClaudeCode, Some("foo"), ts(0), None);
    let threads = cluster(&[s], ClusterOptions::default());
    assert_eq!(threads[0].session_refs, vec!["claude-code/abc-123"]);
}

#[test]
fn thread_id_is_stable_across_calls() {
    let s = mk_session("abc", Provider::ClaudeCode, Some("foo"), ts(0), None);
    let a = cluster(std::slice::from_ref(&s), ClusterOptions::default());
    let b = cluster(&[s], ClusterOptions::default());
    assert_eq!(a[0].id, b[0].id);
}

#[test]
fn thread_id_changes_when_project_changes() {
    let s_foo = mk_session("abc", Provider::ClaudeCode, Some("foo"), ts(0), None);
    let s_bar = mk_session("abc", Provider::ClaudeCode, Some("bar"), ts(0), None);
    let a = cluster(&[s_foo], ClusterOptions::default());
    let b = cluster(&[s_bar], ClusterOptions::default());
    assert_ne!(a[0].id, b[0].id);
}

#[test]
fn cross_provider_same_project_merges_when_close() {
    let sessions = vec![
        mk_session("a", Provider::ClaudeCode, Some("foo"), ts(0), Some(ts(60))),
        mk_session("b", Provider::CodexCli, Some("foo"), ts(120), Some(ts(180))),
    ];
    let threads = cluster(&sessions, ClusterOptions::default());
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].providers, vec!["claude-code", "codex-cli"]);
}

#[test]
fn branches_collected_dedup_sorted() {
    let mut s1 = mk_session("a", Provider::ClaudeCode, Some("foo"), ts(0), None);
    s1.git_branch = Some("main".to_string());
    let mut s2 = mk_session("b", Provider::ClaudeCode, Some("foo"), ts(60), None);
    s2.git_branch = Some("feature/x".to_string());
    let mut s3 = mk_session("c", Provider::ClaudeCode, Some("foo"), ts(120), None);
    s3.git_branch = Some("main".to_string());
    let threads = cluster(&[s1, s2, s3], ClusterOptions::default());
    assert_eq!(threads[0].branches, vec!["feature/x", "main"]);
}

#[test]
fn empty_input_returns_empty() {
    let threads = cluster(&[], ClusterOptions::default());
    assert!(threads.is_empty());
}

#[test]
fn boundary_at_exact_gap_clusters() {
    let opts = ClusterOptions {
        gap: Duration::seconds(60),
        min_sessions: 1,
    };
    let sessions = vec![
        mk_session("a", Provider::ClaudeCode, Some("foo"), ts(0), Some(ts(0))),
        mk_session("b", Provider::ClaudeCode, Some("foo"), ts(60), None),
    ];
    let threads = cluster(&sessions, opts);
    assert_eq!(threads.len(), 1);
}

#[test]
fn just_past_gap_does_not_cluster() {
    let opts = ClusterOptions {
        gap: Duration::seconds(60),
        min_sessions: 1,
    };
    let sessions = vec![
        mk_session("a", Provider::ClaudeCode, Some("foo"), ts(0), Some(ts(0))),
        mk_session("b", Provider::ClaudeCode, Some("foo"), ts(61), None),
    ];
    let threads = cluster(&sessions, opts);
    assert_eq!(threads.len(), 2);
}
