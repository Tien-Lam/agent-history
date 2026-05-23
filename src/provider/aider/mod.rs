//! Aider (<https://aider.chat>) provider.
//!
//! Aider is a per-repo CLI coding assistant that writes its conversation
//! transcript to two files in each project directory:
//!
//! - `.aider.chat.history.md` — markdown-formatted conversation log
//! - `.aider.input.history` — raw user input lines. We ignore this; the chat
//!   history file is the canonical record.
//!
//! Because the files live inside each repo (not in a per-user data dir),
//! discovery walks one or more configured roots looking for any directory
//! that contains a `.aider.chat.history.md`.
//!
//! ## Roots, in order
//!
//! 1. `AIDER_ROOT` env var (colon-separated list)
//! 2. `AGHIST_HOME`-rooted `projects/` (testability shim)
//! 3. `~/projects/` (the convention called out in the spec)
//!
//! Walks are bounded to depth 4 — Aider history sits at the project root,
//! so deeper traversal is wasted work and risks dragging in `node_modules` /
//! `.git` blobs from sibling repos.
//!
//! ## File format
//!
//! - `# aider chat started at <ts>` opens a new session.
//! - Lines starting with `####` open a user message; subsequent `####`
//!   lines extend it until the next role boundary.
//! - Plain lines (including fenced code blocks) form the assistant message.
//! - Lines starting with `>` are aider command output / metadata. We
//!   surface them as `Role::Tool` so they're searchable but visually
//!   separable from real conversation.
//!
//! Multiple sessions can share one file; we keep them as distinct `Session`
//! records keyed `<sha8>:<timestamp>` so the IDs are deterministic and stable.

use std::path::{Path, PathBuf};

mod parse;

use super::{
    discovery_error, HistoryProvider, ProviderError, ProviderMessageLoad, ProviderParseStats,
};
use crate::model::{Message, Provider, Session};
use parse::{load_messages_from_file, parse_sessions_in_file};

const HISTORY_FILE: &str = ".aider.chat.history.md";
const MAX_WALK_DEPTH: usize = 4;

pub struct AiderProvider {
    dirs: Vec<PathBuf>,
}

impl AiderProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|p| p.exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

fn base_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();

    if let Ok(roots) = std::env::var("AIDER_ROOT") {
        for r in roots.split(':').filter(|s| !s.is_empty()) {
            result.push(PathBuf::from(r));
        }
    }

    if let Some(home) = super::home_dir() {
        // Spec: "walk ~/projects/". This is also where AGHIST_HOME tests
        // place fixture project trees.
        let projects = home.join("projects");
        if !result.iter().any(|p| p == &projects) {
            result.push(projects);
        }
    }

    result
}

impl HistoryProvider for AiderProvider {
    fn provider(&self) -> Provider {
        Provider::Aider
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();
        for base in &self.dirs {
            if !base.exists() {
                continue;
            }
            let mut history_files = Vec::new();
            collect_history_files(base, 0, &mut history_files)?;
            for file in history_files {
                match parse_sessions_in_file(&file) {
                    Ok(found) => sessions.extend(found),
                    Err(e) => {
                        tracing::warn!(path = %file.display(), error = %e, "skipping unreadable Aider history");
                    }
                }
            }
        }
        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        Ok(self.load_messages_with_stats(session)?.messages)
    }

    fn load_messages_with_stats(
        &self,
        session: &Session,
    ) -> Result<ProviderMessageLoad, ProviderError> {
        let messages =
            load_messages_from_file(&session.source_path, session.started_at, &session.id.0)?;
        Ok(ProviderMessageLoad {
            parse_stats: ProviderParseStats::clean_records(messages.len()),
            messages,
        })
    }
}

/// Recursively scans `dir` for `.aider.chat.history.md` files. Bounded by
/// [`MAX_WALK_DEPTH`] because Aider history lives at project roots — going
/// deeper just wades into `node_modules`, `.git`, etc.
fn collect_history_files(
    dir: &Path,
    depth: usize,
    out: &mut Vec<PathBuf>,
) -> Result<(), ProviderError> {
    if depth > MAX_WALK_DEPTH {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(discovery_error("Aider"))?;
    for entry in entries {
        let entry = entry.map_err(discovery_error("Aider"))?;
        let path = entry.path();
        let ft = entry.file_type().map_err(discovery_error("Aider"))?;
        if ft.is_file() {
            if path.file_name().and_then(|n| n.to_str()) == Some(HISTORY_FILE) {
                out.push(path);
            }
            continue;
        }
        if ft.is_dir() {
            // Skip well-known noise dirs to keep the walk cheap. We don't
            // bother filtering by `.gitignore` — that's overkill for a TUI.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if matches!(
                    name,
                    "node_modules" | ".git" | "target" | "dist" | "build" | ".venv" | "venv"
                ) {
                    continue;
                }
            }
            collect_history_files(&path, depth + 1, out)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
