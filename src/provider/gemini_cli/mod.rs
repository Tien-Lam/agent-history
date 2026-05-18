use std::path::PathBuf;

mod parse;

use super::{HistoryProvider, ProviderError};
use crate::model::{Message, Provider, Session};
use parse::{build_session_from_file, load_messages_from_path, load_project_map};

pub struct GeminiCliProvider {
    dirs: Vec<PathBuf>,
}

impl GeminiCliProvider {
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
        result.push(home.join(".gemini"));
    }
    result
}

impl HistoryProvider for GeminiCliProvider {
    fn provider(&self) -> Provider {
        Provider::GeminiCli
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();

        for base in &self.dirs {
            // Load project name mapping
            let project_map = load_project_map(base);

            // Scan tmp/{project}/chats/session-*.json
            let tmp_dir = base.join("tmp");
            if !tmp_dir.exists() {
                continue;
            }

            let project_dirs =
                std::fs::read_dir(&tmp_dir).map_err(|e| ProviderError::Discovery {
                    provider: "Gemini CLI",
                    source: e,
                })?;

            for project_entry in project_dirs.flatten() {
                if !project_entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let project_slug = project_entry.file_name().to_string_lossy().to_string();

                let chats_dir = project_entry.path().join("chats");
                if !chats_dir.exists() {
                    continue;
                }

                let chat_files =
                    std::fs::read_dir(&chats_dir).map_err(|e| ProviderError::Discovery {
                        provider: "Gemini CLI",
                        source: e,
                    })?;

                for file_entry in chat_files.flatten() {
                    let path = file_entry.path();
                    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                    if !fname.starts_with("session-")
                        || !std::path::Path::new(fname)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
                    {
                        continue;
                    }

                    if let Some(session) =
                        build_session_from_file(&path, &project_slug, &project_map)
                    {
                        sessions.push(session);
                    }
                }
            }
        }

        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        load_messages_from_path(&session.source_path)
    }
}
