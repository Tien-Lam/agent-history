use std::path::PathBuf;

mod discovery;
mod parse;

use super::{HistoryProvider, ProviderError, ProviderMessageLoad, ProviderParseStats};
use crate::model::{Message, Provider, Session};
use discovery::{base_dirs, discover_sessions};
use parse::parse_message_file_with_stats;

pub struct OpenCodeProvider {
    dirs: Vec<PathBuf>,
}

impl OpenCodeProvider {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| d.exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }
}

impl HistoryProvider for OpenCodeProvider {
    fn provider(&self) -> Provider {
        Provider::OpenCode
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
        // source_path points to the storage base dir, session id is in session.id
        // Messages are in message/{sessionID}/msg_*.json
        let message_dir = session.source_path.join("message").join(&session.id.0);
        let part_dir = session.source_path.join("part");
        tracing::debug!(message_dir = %message_dir.display(), "loading OpenCode messages");
        if !message_dir.exists() {
            tracing::warn!(message_dir = %message_dir.display(), "message directory does not exist");
            return Ok(ProviderMessageLoad::from_messages(Vec::new()));
        }

        let mut messages = Vec::new();
        let mut parse_stats = ProviderParseStats::default();
        let files = std::fs::read_dir(&message_dir)?;

        for file_entry in files {
            let file_entry = file_entry?;
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            if let Some(msg) = parse_message_file_with_stats(&path, &part_dir, &mut parse_stats) {
                messages.push(msg);
            }
        }

        messages.sort_by_key(|m| m.timestamp);
        tracing::info!(
            message_dir = %message_dir.display(),
            files = parse_stats.records_seen,
            parse_errors = parse_stats.parse_errors,
            skipped_records = parse_stats.skipped_records,
            empty_content = parse_stats.empty_content,
            messages = messages.len(),
            "OpenCode message loading complete"
        );
        Ok(ProviderMessageLoad {
            messages,
            parse_stats,
        })
    }
}
