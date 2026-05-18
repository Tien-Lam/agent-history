use std::path::{Path, PathBuf};

mod parse;

use super::{HistoryProvider, ProviderError};
use crate::model::{Message, Provider, Session};
pub use crate::provider::text_blocks::parse_text_with_code_blocks;
use parse::{
    build_session_metadata, decode_project_name, parse_history_index, parse_session_messages,
};

pub struct ClaudeCodeProvider {
    dirs: Vec<PathBuf>,
}

impl ClaudeCodeProvider {
    pub fn detect() -> Option<Self> {
        let dirs = base_dirs();
        if dirs.iter().any(|d| d.exists()) {
            Some(Self { dirs })
        } else {
            None
        }
    }

    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }
}

fn base_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Some(home) = super::home_dir() {
        result.push(home.join(".claude"));
    }
    result
}

fn projects_dir(base: &Path) -> PathBuf {
    base.join("projects")
}

impl HistoryProvider for ClaudeCodeProvider {
    fn provider(&self) -> Provider {
        Provider::ClaudeCode
    }

    fn base_dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let mut sessions = Vec::new();

        for base in &self.dirs {
            let history_path = base.join("history.jsonl");
            if !history_path.exists() {
                continue;
            }

            // Build a map of sessionId -> history entries for metadata
            let history_entries = parse_history_index(&history_path)?;

            // Scan project directories for .jsonl session files
            let projects = projects_dir(base);
            if !projects.exists() {
                continue;
            }

            let project_dirs =
                std::fs::read_dir(&projects).map_err(|e| ProviderError::Discovery {
                    provider: "Claude Code",
                    source: e,
                })?;

            for project_entry in project_dirs.flatten() {
                if !project_entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let project_dir = project_entry.path();
                let project_name =
                    decode_project_name(project_entry.file_name().to_string_lossy().as_ref());

                let entries =
                    std::fs::read_dir(&project_dir).map_err(|e| ProviderError::Discovery {
                        provider: "Claude Code",
                        source: e,
                    })?;

                for file_entry in entries.flatten() {
                    let path = file_entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                        continue;
                    }

                    let session_id = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();

                    if let Some(session) =
                        build_session_metadata(&path, &session_id, &project_name, &history_entries)
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
        parse_session_messages(&session.source_path)
    }
}
