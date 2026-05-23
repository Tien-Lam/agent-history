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

use std::path::PathBuf;

mod discovery;
mod parse;

use super::{HistoryProvider, ProviderError, ProviderMessageLoad};
use crate::model::{Message, Provider, Session};
use discovery::{base_dirs, discover_sessions, SESSIONS_SUBDIR};
use parse::{parse_jsonl, parse_jsonl_with_stats};

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

// ── Provider impl ─────────────────────────────────────────────────────────────

impl HistoryProvider for ContinueDevProvider {
    fn provider(&self) -> Provider {
        Provider::ContinueDev
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        discover_sessions(&self.dirs)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        parse_jsonl(&session.source_path, &session.started_at).map_err(|reason| {
            ProviderError::Parse {
                path: session.source_path.clone(),
                reason,
            }
        })
    }

    fn load_messages_with_stats(
        &self,
        session: &Session,
    ) -> Result<ProviderMessageLoad, ProviderError> {
        parse_jsonl_with_stats(&session.source_path, &session.started_at).map_err(|reason| {
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
