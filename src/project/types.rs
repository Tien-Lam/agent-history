use chrono::{DateTime, Utc};
use serde::Serialize;

use super::extract::{DecisionRow, TodoRow};
use super::files::FileTouch;
use super::tokens::ProjectTokens;
use crate::threads::Thread;

/// Per-section caps. Zero means "no cap" for the corresponding section.
#[derive(Debug, Clone, Copy)]
pub struct ProjectLimits {
    pub decisions: usize,
    pub todos: usize,
    pub threads: usize,
    pub files: usize,
}

impl ProjectLimits {
    /// Defaults sized for a quick TTY scan: a handful of headlines per
    /// section, not an exhaustive dump.
    pub const DEFAULTS: Self = Self {
        decisions: 5,
        todos: 10,
        threads: 5,
        files: 10,
    };
}

impl Default for ProjectLimits {
    fn default() -> Self {
        Self::DEFAULTS
    }
}

/// The per-project dashboard envelope. Section caps are applied during
/// aggregation; raw counts live in [`ProjectMeta`] so callers can tell
/// "5 of 23" from "5 of 5".
#[derive(Debug, Clone, Serialize)]
pub struct ProjectReport {
    pub query: String,
    pub matched_projects: Vec<String>,
    pub session_count: usize,
    pub message_count: usize,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub token_usage: ProjectTokens,
    pub decisions: Vec<DecisionRow>,
    pub todos: Vec<TodoRow>,
    pub threads: Vec<Thread>,
    pub top_files: Vec<FileTouch>,
    /// 24 buckets, index = UTC hour 0..23, value = message count in that hour.
    pub time_of_day: [u64; 24],
    pub meta: ProjectMeta,
}

/// Companion totals for the section caps. Each `*_total` is the count
/// before truncation by [`ProjectLimits`].
#[derive(Debug, Clone, Serialize)]
pub struct ProjectMeta {
    pub limits: ProjectLimitsView,
    pub decisions_total: usize,
    pub todos_total: usize,
    pub threads_total: usize,
    pub files_total: usize,
    pub thread_gap_hours: i64,
    pub decisions_threshold: f32,
}

/// Plain `Serialize`-able view of [`ProjectLimits`].
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ProjectLimitsView {
    pub decisions: usize,
    pub todos: usize,
    pub threads: usize,
    pub files: usize,
}

impl From<ProjectLimits> for ProjectLimitsView {
    fn from(l: ProjectLimits) -> Self {
        Self {
            decisions: l.decisions,
            todos: l.todos,
            threads: l.threads,
            files: l.files,
        }
    }
}
