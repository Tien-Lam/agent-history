//! Cline (VS Code extension, formerly Claude Dev) provider.
//!
//! Cline (<https://github.com/cline/cline>) is a VS Code extension that stores
//! each task's conversation in a directory under VS Code's extension global
//! storage. On disk:
//!
//! - Linux:   `~/.config/Code/User/globalStorage/saoudrizwan.claude-dev/tasks/<ts>/`
//! - macOS:   `~/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/tasks/<ts>/`
//! - Windows: `%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\tasks\<ts>\`
//!
//! Cursor and Windsurf ship VS Code forks with the same extension; their
//! paths substitute `Cursor` or `Windsurf` for `Code`. We probe all three.
//!
//! Each task directory is named by a Unix timestamp in milliseconds (e.g.
//! `1698765432000`) and contains:
//!
//! - `api_conversation_history.json` — Anthropic Messages API array (canonical)
//! - `ui_messages.json` — UI-layer messages (we use the first entry's `text`
//!   as the session summary)
//! - `task_metadata.json` (newer Cline): `{createdAt, updatedAt, ...}` timestamps
//!
//! `api_conversation_history.json` follows the Anthropic Messages API shape:
//!
//! ```json
//! [
//!   {"role": "user",      "content": [{"type": "text", "text": "..."}]},
//!   {"role": "assistant", "content": [{"type": "text", "text": "..."}]},
//!   ...
//! ]
//! ```
//!
//! Content blocks may also include `tool_use` / `tool_result` entries.
//! Unknown block types and unknown roles are silently skipped.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};

mod parse;

use super::{HistoryProvider, ProviderError, ProviderMessageLoad};
use crate::model::{Message, Provider, Session};
use parse::{parse_api_history_with_stats, parse_task_dir, API_HISTORY_FILE};

const EXTENSION_ID: &str = "saoudrizwan.claude-dev";
const TASKS_SUBDIR: &str = "tasks";

pub struct ClineProvider {
    dirs: Vec<PathBuf>,
}

impl ClineProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| tasks_dir(d).exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

fn tasks_dir(base: &Path) -> PathBuf {
    base.join(EXTENSION_ID).join(TASKS_SUBDIR)
}

/// VS Code (and fork) global-storage directories, each being the parent that
/// holds `saoudrizwan.claude-dev/tasks/`. We check VS Code, Cursor, and
/// Windsurf. Honors `CLINE_HOME` for testability.
fn base_dirs() -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = Vec::new();

    if let Ok(cline_home) = std::env::var("CLINE_HOME") {
        result.push(PathBuf::from(cline_home));
        return result;
    }

    if let Some(home) = super::home_dir() {
        for editor in &["Code", "Cursor", "Windsurf"] {
            result.push(
                home.join(".config")
                    .join(editor)
                    .join("User")
                    .join("globalStorage"),
            );
            result.push(
                home.join("Library")
                    .join("Application Support")
                    .join(editor)
                    .join("User")
                    .join("globalStorage"),
            );
            result.push(
                home.join("AppData")
                    .join("Roaming")
                    .join(editor)
                    .join("User")
                    .join("globalStorage"),
            );
        }
    }

    result
}

// ── Provider impl ─────────────────────────────────────────────────────────────

impl HistoryProvider for ClineProvider {
    fn provider(&self) -> Provider {
        Provider::Cline
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

        for base in &self.dirs {
            let td = tasks_dir(base);
            if !td.is_dir() {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&td) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                if !seen.insert(canonical) {
                    continue;
                }
                if let Some(session) = parse_task_dir(&path) {
                    sessions.push(session);
                }
            }
        }

        sessions.sort_by_key(|s| Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        Ok(self.load_messages_with_stats(session)?.messages)
    }

    fn load_messages_with_stats(
        &self,
        session: &Session,
    ) -> Result<ProviderMessageLoad, ProviderError> {
        let history_path = session.source_path.join(API_HISTORY_FILE);
        parse_api_history_with_stats(&history_path, &session.started_at).map_err(|reason| {
            ProviderError::Parse {
                path: history_path.clone(),
                reason,
            }
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
