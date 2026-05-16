use std::path::PathBuf;

use super::{HistoryProvider, ProviderError};
use crate::model::{Message, Provider, Session};
use crate::provider::opencode_parse::{build_session_from_file, parse_message_file};

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
                std::fs::read_dir(&session_dir).map_err(|e| ProviderError::Discovery {
                    provider: "OpenCode",
                    source: e,
                })?;

            for project_entry in project_dirs.flatten() {
                if !project_entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let Ok(files) = std::fs::read_dir(project_entry.path()) else {
                    continue;
                };

                for file_entry in files.flatten() {
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
        // source_path points to the storage base dir, session id is in session.id
        // Messages are in message/{sessionID}/msg_*.json
        let message_dir = session.source_path.join("message").join(&session.id.0);
        let part_dir = session.source_path.join("part");
        tracing::debug!(message_dir = %message_dir.display(), "loading OpenCode messages");
        if !message_dir.exists() {
            tracing::warn!(message_dir = %message_dir.display(), "message directory does not exist");
            return Ok(Vec::new());
        }

        let mut messages = Vec::new();
        let mut file_count: usize = 0;
        let mut parse_failures: usize = 0;
        let files = std::fs::read_dir(&message_dir)?;

        for file_entry in files.flatten() {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            file_count += 1;

            if let Some(msg) = parse_message_file(&path, &part_dir) {
                messages.push(msg);
            } else {
                parse_failures += 1;
                tracing::warn!(path = %path.display(), "failed to parse message file");
            }
        }

        messages.sort_by_key(|m| m.timestamp);
        tracing::info!(
            message_dir = %message_dir.display(),
            files = file_count,
            parse_failures,
            messages = messages.len(),
            "OpenCode message loading complete"
        );
        Ok(messages)
    }
}
