use std::path::PathBuf;

mod parse;

use super::{HistoryProvider, ProviderError, ProviderMessageLoad};
use crate::model::{Message, Provider, Session};
use parse::{
    build_session, parse_checkpoint_md, parse_events_jsonl, parse_events_jsonl_with_stats,
};

pub struct CopilotCliProvider {
    dirs: Vec<PathBuf>,
}

impl CopilotCliProvider {
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

fn base_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Some(home) = super::home_dir() {
        result.push(home.join(".copilot").join("session-state"));
    }
    result
}

impl HistoryProvider for CopilotCliProvider {
    fn provider(&self) -> Provider {
        Provider::CopilotCli
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

            let session_dirs = std::fs::read_dir(base).map_err(|e| ProviderError::Discovery {
                provider: "Copilot CLI",
                source: e,
            })?;

            for entry in session_dirs.flatten() {
                if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let session_dir = entry.path();
                let workspace_path = session_dir.join("workspace.yaml");

                if !workspace_path.exists() {
                    continue;
                }

                if let Some(session) = build_session(&session_dir, &workspace_path) {
                    sessions.push(session);
                }
            }
        }

        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        // Look for events.jsonl in the session directory
        let events_path = session.source_path.join("events.jsonl");
        if events_path.exists() {
            parse_events_jsonl(&events_path)
        } else {
            // Fall back to checkpoint markdown
            let checkpoint_path = session.source_path.join("checkpoints").join("index.md");
            if checkpoint_path.exists() {
                parse_checkpoint_md(&checkpoint_path)
            } else {
                Ok(Vec::new())
            }
        }
    }

    fn load_messages_with_stats(
        &self,
        session: &Session,
    ) -> Result<ProviderMessageLoad, ProviderError> {
        let events_path = session.source_path.join("events.jsonl");
        if events_path.exists() {
            parse_events_jsonl_with_stats(&events_path)
        } else {
            self.load_messages(session)
                .map(ProviderMessageLoad::from_messages)
        }
    }
}
