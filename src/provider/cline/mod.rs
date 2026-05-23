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

use std::path::PathBuf;

mod discovery;
mod parse;

use super::{HistoryProvider, ProviderError, ProviderMessageLoad};
use crate::model::{Message, Provider, Session};
use discovery::{base_dirs, discover_sessions, tasks_dir};
use parse::{parse_api_history_with_stats, API_HISTORY_FILE};

#[cfg(test)]
use discovery::{EXTENSION_ID, TASKS_SUBDIR};

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

// ── Provider impl ─────────────────────────────────────────────────────────────

impl HistoryProvider for ClineProvider {
    fn provider(&self) -> Provider {
        Provider::Cline
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        discover_sessions(&self.dirs)
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
