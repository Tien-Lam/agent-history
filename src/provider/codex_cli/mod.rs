use std::path::{Path, PathBuf};

mod parse;

use super::{
    discovery_error, entry_is_regular_file, HistoryProvider, ProviderError, ProviderMessageLoad,
};
use crate::model::{Message, Provider, Session};
use parse::{
    build_session_from_rollout, parse_rollout_messages, parse_rollout_messages_with_stats,
};

pub struct CodexCliProvider {
    dirs: Vec<PathBuf>,
}

impl CodexCliProvider {
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
        result.push(home.join(".codex").join("sessions"));
    }
    if let Some(codex_home) = super::env_path("CODEX_HOME") {
        result.push(codex_home.join("sessions"));
    }
    result
}

impl HistoryProvider for CodexCliProvider {
    fn provider(&self) -> Provider {
        Provider::CodexCli
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

            // Scan {YYYY}/{MM}/{DD}/rollout-*.jsonl
            collect_rollout_files(base, &mut sessions)?;
        }

        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        parse_rollout_messages(&session.source_path)
    }

    fn load_messages_with_stats(
        &self,
        session: &Session,
    ) -> Result<ProviderMessageLoad, ProviderError> {
        parse_rollout_messages_with_stats(&session.source_path)
    }
}

fn collect_rollout_files(base: &Path, sessions: &mut Vec<Session>) -> Result<(), ProviderError> {
    // Walk year/month/day directories
    let years = std::fs::read_dir(base).map_err(discovery_error("Codex CLI"))?;

    for year_entry in years {
        let year_entry = year_entry.map_err(discovery_error("Codex CLI"))?;
        if !year_entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }

        let months = std::fs::read_dir(year_entry.path()).map_err(discovery_error("Codex CLI"))?;

        for month_entry in months {
            let month_entry = month_entry.map_err(discovery_error("Codex CLI"))?;
            if !month_entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }

            let days =
                std::fs::read_dir(month_entry.path()).map_err(discovery_error("Codex CLI"))?;

            for day_entry in days {
                let day_entry = day_entry.map_err(discovery_error("Codex CLI"))?;
                if !day_entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let files =
                    std::fs::read_dir(day_entry.path()).map_err(discovery_error("Codex CLI"))?;

                for file_entry in files {
                    let file_entry = file_entry.map_err(discovery_error("Codex CLI"))?;
                    if !entry_is_regular_file(&file_entry) {
                        continue;
                    }
                    let path = file_entry.path();
                    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                    if fname.starts_with("rollout-")
                        && std::path::Path::new(fname)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
                    {
                        if let Some(session) = build_session_from_rollout(&path) {
                            sessions.push(session);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
