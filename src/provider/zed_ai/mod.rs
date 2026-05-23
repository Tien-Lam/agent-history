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

mod parse;

use super::{discovery_error, HistoryProvider, ProviderError, ProviderMessageLoad};
use crate::model::{Message, Provider, Session};
use parse::{load_messages_from_path_with_stats, read_session};

const CONVERSATIONS_SUBDIR: &str = "conversations";

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

/// Zed base directories, ordered by likelihood. Each entry is the parent
/// directory that holds `conversations/`. Honors `AGHIST_HOME` (for tests)
/// and `ZED_HOME` as a power-user override.
fn base_dirs() -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = Vec::new();

    if let Ok(zed_home) = std::env::var("ZED_HOME") {
        result.push(PathBuf::from(zed_home));
    }

    if let Some(home) = super::home_dir() {
        // Linux (XDG data): ~/.local/share/zed
        result.push(home.join(".local").join("share").join("zed"));
        // Linux (XDG config): ~/.config/zed — older Zed builds wrote here
        result.push(home.join(".config").join("zed"));
        // macOS: ~/Library/Application Support/Zed
        result.push(home.join("Library").join("Application Support").join("Zed"));
        // Windows: %APPDATA%\Zed (mirrored under home for AGHIST_HOME tests)
        result.push(home.join("AppData").join("Roaming").join("Zed"));
    }

    if std::env::var("AGHIST_HOME").is_err() {
        if let Some(base) = directories::BaseDirs::new() {
            let appdata = base.config_dir().join("Zed");
            if !result.iter().any(|p| p == &appdata) {
                result.push(appdata);
            }
            let data = base.data_dir().join("Zed");
            if !result.iter().any(|p| p == &data) {
                result.push(data);
            }
        }
    }

    result
}

impl HistoryProvider for ZedAiProvider {
    fn provider(&self) -> Provider {
        Provider::ZedAi
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

        for base in &self.dirs {
            let conv_dir = base.join(CONVERSATIONS_SUBDIR);
            if !conv_dir.exists() {
                continue;
            }
            let entries = std::fs::read_dir(&conv_dir).map_err(discovery_error("Zed AI"))?;
            for entry in entries {
                let entry = entry.map_err(discovery_error("Zed AI"))?;
                let path = entry.path();
                if path.extension().is_none_or(|x| x != "json") {
                    continue;
                }
                let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                if !seen.insert(canonical) {
                    continue;
                }
                match read_session(&path) {
                    Ok(Some(s)) => sessions.push(s),
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "skipping unreadable Zed conversation");
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
        load_messages_from_path_with_stats(&session.source_path)
    }
}

#[cfg(test)]
mod tests;
