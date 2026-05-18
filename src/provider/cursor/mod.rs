//! Cursor (the AI editor, <https://cursor.com>) provider.
//!
//! Cursor stores its composer/chat history in a `SQLite` database under
//! `<config>/User/globalStorage/state.vscdb`. The relevant table is
//! `cursorDiskKV (key TEXT, value BLOB)` where:
//!
//! - `composerData:<composerId>`  → JSON session header
//! - `bubbleId:<composerId>:<bubbleId>` → JSON message bubble
//!
//! The bubble `type` field encodes the role: `1` = user, `2` = assistant.
//! Tool calls live under `toolFormerData` (legacy) or `tools`/`toolCalls`
//! (newer schema variants); we try both.
//!
//! Cursor has shipped several format variants — fields are tolerant: missing
//! fields skip rather than fail, and unparseable values are logged but never
//! crash discovery.

mod format;
mod message;
mod store;

use std::path::PathBuf;

use super::{HistoryProvider, ProviderError};
use crate::model::{Message, Provider, Session};
use store::{load_messages_from_db, read_sessions, state_db_path};

pub struct CursorProvider {
    dirs: Vec<PathBuf>,
}

impl CursorProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| state_db_path(d).exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

/// Cursor base directories, ordered by likelihood. Each entry is the parent
/// directory holding `User/globalStorage/state.vscdb`. Honors `AGHIST_HOME`
/// for testability and `CURSOR_HOME` as a power-user override.
fn base_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();

    if let Ok(cursor_home) = std::env::var("CURSOR_HOME") {
        result.push(PathBuf::from(cursor_home));
    }

    if let Some(home) = super::home_dir() {
        // Linux: ~/.config/Cursor
        result.push(home.join(".config").join("Cursor"));
        // macOS: ~/Library/Application Support/Cursor
        result.push(
            home.join("Library")
                .join("Application Support")
                .join("Cursor"),
        );
        // Windows: %APPDATA%\Cursor (mirrored under home for AGHIST_HOME tests)
        result.push(home.join("AppData").join("Roaming").join("Cursor"));
    }

    if std::env::var("AGHIST_HOME").is_err() {
        if let Some(base) = directories::BaseDirs::new() {
            let appdata = base.config_dir().join("Cursor");
            if !result.iter().any(|p| p == &appdata) {
                result.push(appdata);
            }
        }
    }

    result
}

impl HistoryProvider for CursorProvider {
    fn provider(&self) -> Provider {
        Provider::Cursor
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();

        for base in &self.dirs {
            let db_path = state_db_path(base);
            if !db_path.exists() {
                continue;
            }

            match read_sessions(&db_path) {
                Ok(found) => sessions.extend(found),
                Err(e) => {
                    tracing::warn!(path = %db_path.display(), error = %e, "skipping unreadable Cursor db");
                }
            }
        }

        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        // source_path points to the state.vscdb file itself — set by discover_sessions.
        load_messages_from_db(&session.source_path, &session.id.0)
    }
}

#[cfg(test)]
mod tests;
