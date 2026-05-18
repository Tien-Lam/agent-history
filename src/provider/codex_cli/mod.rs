use std::path::{Path, PathBuf};

mod parse;

use super::{HistoryProvider, ProviderError};
use crate::model::{Message, Provider, Session};
use parse::{build_session_from_rollout, parse_rollout_messages};

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
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        result.push(PathBuf::from(codex_home).join("sessions"));
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
            collect_rollout_files(base, &mut sessions);
        }

        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        Ok(sessions)
    }

    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError> {
        parse_rollout_messages(&session.source_path)
    }
}

fn collect_rollout_files(base: &Path, sessions: &mut Vec<Session>) {
    // Walk year/month/day directories
    let Ok(years) = std::fs::read_dir(base) else {
        return;
    };

    for year_entry in years.flatten() {
        if !year_entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }

        let Ok(months) = std::fs::read_dir(year_entry.path()) else {
            continue;
        };

        for month_entry in months.flatten() {
            if !month_entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }

            let Ok(days) = std::fs::read_dir(month_entry.path()) else {
                continue;
            };

            for day_entry in days.flatten() {
                if !day_entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }

                let Ok(files) = std::fs::read_dir(day_entry.path()) else {
                    continue;
                };

                for file_entry in files.flatten() {
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
}
