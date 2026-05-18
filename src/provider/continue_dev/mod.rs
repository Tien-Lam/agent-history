//! Continue.dev VS Code/JetBrains extension provider.
//!
//! Continue (<https://continue.dev>) stores each chat session as a JSONL file
//! under `~/.continue/sessions/`. On disk:
//!
//! - All platforms: `~/.continue/sessions/<uuid>.jsonl`
//!
//! An optional `~/.continue/sessions/index.json` contains a session index with
//! titles and timestamps, used when present to enrich session metadata.
//!
//! Each JSONL file has one JSON object per line:
//!
//! ```json
//! {"role": "user", "content": "message text"}
//! {"role": "assistant", "content": "response text"}
//! ```
//!
//! Content can be a plain string or an array of Anthropic-style content blocks
//! (`{"type": "text", "text": "..."}` etc.). Both forms are handled.
//!
//! The session ID is the JSONL filename stem (UUID or similar). Timestamps are
//! derived from the index file when available, falling back to file mtime.

use std::cmp::Reverse;
use std::path::PathBuf;

mod parse;

use super::{HistoryProvider, ProviderError};
use crate::model::{Message, Provider, Session};
use parse::{build_session_from_file, load_index, parse_jsonl};

const SESSIONS_SUBDIR: &str = "sessions";

pub struct ContinueDevProvider {
    dirs: Vec<PathBuf>,
}

impl ContinueDevProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| d.join(SESSIONS_SUBDIR).exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

fn base_dirs() -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = Vec::new();

    if let Ok(continue_home) = std::env::var("CONTINUE_HOME") {
        result.push(PathBuf::from(continue_home));
        return result;
    }

    if let Some(home) = super::home_dir() {
        result.push(home.join(".continue"));
    }

    result
}

// ── Provider impl ─────────────────────────────────────────────────────────────

impl HistoryProvider for ContinueDevProvider {
    fn provider(&self) -> Provider {
        Provider::ContinueDev
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

        for base in &self.dirs {
            let sessions_dir = base.join(SESSIONS_SUBDIR);
            if !sessions_dir.is_dir() {
                continue;
            }

            // Load index for enriched metadata (optional)
            let index = load_index(&sessions_dir);

            let Ok(entries) = std::fs::read_dir(&sessions_dir) else {
                continue;
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let Some(ext) = path.extension() else {
                    continue;
                };
                if ext != "jsonl" {
                    continue;
                }
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                if !seen.insert(canonical) {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let session_id = stem.to_string();
                sessions.push(build_session_from_file(path, session_id, index.as_deref()));
            }
        }

        sessions.sort_by_key(|s| Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        parse_jsonl(&session.source_path, &session.started_at).map_err(|reason| {
            ProviderError::Parse {
                path: session.source_path.clone(),
                reason,
            }
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
