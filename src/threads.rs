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
            let close_to_prev =
                cur_end.is_some_and(|end| s.started_at.signed_duration_since(end) <= opts.gap);
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
    debug_assert!(
        !sessions.is_empty(),
        "thread must have at least one session"
    );

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
        .map(|s| s.session_ref().to_string())
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
mod tests;
