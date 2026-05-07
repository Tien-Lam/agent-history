//! Cluster sessions into "threads" of related work.
//!
//! A *thread* groups sessions that look like the same engagement: same
//! project, time-adjacent (a coffee-break-sized gap, not weeks), so that
//! "what's the history on feature X" can be answered without sifting
//! every Claude Code session by hand.
//!
//! ## Heuristic
//!
//! 1. Bucket by `project_name` (sessions without a project share an
//!    "(unknown)" bucket — they still cluster, just less informatively).
//! 2. Within each bucket, sort by `started_at` ascending and walk: a
//!    session joins the current thread if its `started_at` is within
//!    `gap` of the previous session's `ended_at` (or `started_at` when
//!    no end is recorded). Otherwise it starts a new thread.
//!
//! This is the dumbest thing that could work and is exactly what the
//! spike asked for. Clustering on shared file paths or branch
//! similarity can layer on top later — the output shape supports it.
//!
//! Thread IDs are derived from `(project, first_session_ref)` via FNV-1a
//! so they're deterministic across runs and stable as long as the first
//! session of a thread doesn't change. They're not cryptographic — just
//! "this looks the same as last time you ran it."

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::model::Session;

/// Default time gap (4 hours): consecutive sessions in the same project
/// within this window cluster into one thread. Half a working day mirrors
/// "I picked this back up after lunch" cadence; longer gaps usually
/// indicate context switches across days.
pub const DEFAULT_GAP_HOURS: i64 = 4;

#[derive(Debug, Clone, Copy)]
pub struct ClusterOptions {
    pub gap: Duration,
    pub min_sessions: usize,
}

impl Default for ClusterOptions {
    fn default() -> Self {
        Self {
            gap: Duration::hours(DEFAULT_GAP_HOURS),
            min_sessions: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Thread {
    pub id: String,
    pub project: Option<String>,
    pub providers: Vec<String>,
    pub session_count: usize,
    pub message_count: usize,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub branches: Vec<String>,
    pub session_refs: Vec<String>,
    pub summary_seed: Option<String>,
}

/// Cluster sessions into threads. See module docs for the heuristic.
///
/// Output is sorted most-recent-first by `started_at`, ties broken by
/// project name ascending so the order is deterministic.
pub fn cluster(sessions: &[Session], opts: ClusterOptions) -> Vec<Thread> {
    let mut buckets: BTreeMap<Option<String>, Vec<&Session>> = BTreeMap::new();
    for s in sessions {
        buckets.entry(s.project_name.clone()).or_default().push(s);
    }

    let mut threads = Vec::new();
    for (project, mut group) in buckets {
        group.sort_by_key(|s| s.started_at);

        let mut cur: Vec<&Session> = Vec::new();
        let mut cur_end: Option<DateTime<Utc>> = None;

        for s in group {
            let close_to_prev = cur_end
                .is_some_and(|end| s.started_at.signed_duration_since(end) <= opts.gap);
            if cur.is_empty() || close_to_prev {
                cur.push(s);
                let s_end = s.ended_at.unwrap_or(s.started_at);
                cur_end = Some(cur_end.map_or(s_end, |e| e.max(s_end)));
            } else {
                if cur.len() >= opts.min_sessions {
                    threads.push(make_thread(project.as_deref(), &cur));
                }
                cur = vec![s];
                cur_end = Some(s.ended_at.unwrap_or(s.started_at));
            }
        }
        if cur.len() >= opts.min_sessions {
            threads.push(make_thread(project.as_deref(), &cur));
        }
    }

    threads.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then(a.project.cmp(&b.project))
    });
    threads
}

fn make_thread(project: Option<&str>, sessions: &[&Session]) -> Thread {
    debug_assert!(!sessions.is_empty(), "thread must have at least one session");

    // Sessions are pre-sorted by started_at ascending in `cluster`.
    let started_at = sessions[0].started_at;
    let ended_at = sessions
        .iter()
        .map(|s| s.ended_at.unwrap_or(s.started_at))
        .max()
        .unwrap_or(started_at);

    let mut providers: Vec<String> = sessions
        .iter()
        .map(|s| s.provider.slug().to_string())
        .collect();
    providers.sort();
    providers.dedup();

    let mut branches: Vec<String> = sessions
        .iter()
        .filter_map(|s| s.git_branch.clone())
        .filter(|b| !b.is_empty())
        .collect();
    branches.sort();
    branches.dedup();

    let session_refs: Vec<String> = sessions
        .iter()
        .map(|s| format!("{}/{}", s.provider.slug(), s.id.0))
        .collect();

    let summary_seed = sessions
        .iter()
        .find_map(|s| s.summary.clone())
        .filter(|s| !s.is_empty());

    let message_count: usize = sessions.iter().map(|s| s.message_count).sum();
    let id = thread_id(project, &session_refs[0]);

    Thread {
        id,
        project: project.map(str::to_string),
        providers,
        session_count: sessions.len(),
        message_count,
        started_at,
        ended_at,
        branches,
        session_refs,
        summary_seed,
    }
}

/// Deterministic short thread id. FNV-1a (64-bit) over
/// `<project>|<first_session_ref>` — collisions are astronomically
/// unlikely for the cardinalities we deal with (thousands of threads
/// per user) and the id is meant for human reference, not crypto.
fn thread_id(project: Option<&str>, first_ref: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in project.unwrap_or("").as_bytes() {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h ^= u64::from(b'|');
    h = h.wrapping_mul(0x100_0000_01b3);
    for byte in first_ref.as_bytes() {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("th-{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Provider, SessionId};
    use std::path::PathBuf;

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
            mk_session("b", Provider::ClaudeCode, Some("foo"), ts(120), Some(ts(180))),
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
        // Most recent first.
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
            // Solo session far away from anything else.
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
            // Exactly at the boundary — should still cluster (<= gap).
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
}
