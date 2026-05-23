//! Zed AI (Zed editor's assistant panel) provider.
//!
//! Zed (<https://zed.dev>) ships an AI assistant panel that saves each
//! conversation as a JSON file under the editor's user-data directory.
//! On disk:
//!
//! - Linux:   `~/.local/share/zed/conversations/*.json` (also `~/.config/zed/conversations/`)
//! - macOS:   `~/Library/Application Support/Zed/conversations/*.json`
//! - Windows: `%APPDATA%\Zed\conversations\*.json`
//!
//! The on-disk schema has shifted across Zed releases — early builds used
//! a `buffer`+`anchor_range` shape, newer builds inline `text` per message.
//! This parser targets the inlined shape and tolerates field churn: unknown
//! roles, missing timestamps, and parse errors all skip rather than abort
//! discovery, matching the project-wide "corrupt session files are skipped,
//! never crash" stance.
//!
//! Expected JSON shape (fields are all optional unless noted):
//!
//! ```json
//! {
//!   "id": "uuid",
//!   "summary": "title text",
//!   "model": "anthropic/claude-sonnet-4",
//!   "workspace": "/abs/path/to/project",
//!   "created_at": "2026-01-01T00:00:00Z",
//!   "updated_at": "2026-01-01T00:05:00Z",
//!   "messages": [
//!     {
//!       "id": "msg-uuid",
//!       "role": "User" | "Assistant" | "System",
//!       "text": "message body",
//!       "timestamp": "2026-01-01T00:00:00Z"
//!     }
//!   ]
//! }
//! ```
//!
//! `created_at` / `updated_at` / `timestamp` accept either an RFC3339 string
//! or epoch milliseconds (`i64`) so we can absorb both common shapes.

use std::path::PathBuf;

mod discovery;
mod parse;

use super::{HistoryProvider, ProviderError, ProviderMessageLoad};
use crate::model::{Message, Provider, Session};
use discovery::{base_dirs, discover_sessions, CONVERSATIONS_SUBDIR};
use parse::load_messages_from_path_with_stats;

pub struct ZedAiProvider {
    dirs: Vec<PathBuf>,
}

impl ZedAiProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| d.join(CONVERSATIONS_SUBDIR).exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

impl HistoryProvider for ZedAiProvider {
    fn provider(&self) -> Provider {
        Provider::ZedAi
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
        load_messages_from_path_with_stats(&session.source_path)
    }
}

#[cfg(test)]
mod tests;
