use std::path::PathBuf;

mod parse;

use super::{
    discovery_error, HistoryProvider, ProviderError, ProviderMessageLoad, ProviderParseStats,
};
use crate::model::{Message, Provider, Session};
use parse::{build_session_from_file, parse_message_file_with_stats};

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

fn base_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();

    if let Some(home) = super::home_dir() {
        // OpenCode commonly stores data at ~/.local/share/opencode/storage/
        // even on Windows, so always check this path
        let local_share = home
            .join(".local")
            .join("share")
            .join("opencode")
            .join("storage");
        result.push(local_share);
    }

    if std::env::var("AGHIST_HOME").is_err() {
        // Also check platform-native data directories
        if let Some(data_dir) =
            directories::ProjectDirs::from("", "", "opencode").map(|d| d.data_dir().to_path_buf())
        {
            let storage = data_dir.join("storage");
            if !result.iter().any(|p| p == &storage) {
                result.push(storage);
            }
        }

        if let Some(base) = directories::BaseDirs::new() {
            let appdata_path = base.data_dir().join("opencode");
            if !result.iter().any(|p| p == &appdata_path) {
                result.push(appdata_path);
            }
        }
    }

    // OPENCODE_DATA_DIR env var
    if let Ok(data_dir) = std::env::var("OPENCODE_DATA_DIR") {
        result.push(PathBuf::from(data_dir));
    }

    result
}

impl HistoryProvider for OpenCodeProvider {
    fn provider(&self) -> Provider {
        Provider::OpenCode
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

            // Scan session/{projectHash}/*.json
            let session_dir = base.join("session");
            if !session_dir.exists() {
                continue;
            }

            let project_dirs =
                std::fs::read_dir(&session_dir).map_err(discovery_error("OpenCode"))?;

            for project_entry in project_dirs {
                let project_entry = project_entry.map_err(discovery_error("OpenCode"))?;
                if !project_entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let files =
                    std::fs::read_dir(project_entry.path()).map_err(discovery_error("OpenCode"))?;

                for file_entry in files {
                    let file_entry = file_entry.map_err(discovery_error("OpenCode"))?;
                    let path = file_entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }

                    if let Some(session) = build_session_from_file(&path, base) {
                        sessions.push(session);
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
