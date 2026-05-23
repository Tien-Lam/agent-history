use std::cmp::Reverse;
use std::collections::HashSet;
use std::path::PathBuf;

use super::super::{discovery_error, ProviderError};
use super::parse::{build_session_from_file, load_index};
use crate::model::Session;

pub(super) const SESSIONS_SUBDIR: &str = "sessions";

pub(super) fn base_dirs() -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = Vec::new();

    if let Ok(continue_home) = std::env::var("CONTINUE_HOME") {
        result.push(PathBuf::from(continue_home));
        return result;
    }

    if let Some(home) = super::super::home_dir() {
        result.push(home.join(".continue"));
    }

    result
}

pub(super) fn discover_sessions(dirs: &[PathBuf]) -> Result<Vec<Session>, ProviderError> {
    let mut sessions = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for base in dirs {
        let sessions_dir = base.join(SESSIONS_SUBDIR);
        if !sessions_dir.is_dir() {
            continue;
        }

        // Load index for enriched metadata (optional).
        let index = load_index(&sessions_dir);
        let entries = std::fs::read_dir(&sessions_dir).map_err(discovery_error("Continue"))?;

        for entry in entries {
            let entry = entry.map_err(discovery_error("Continue"))?;
            let path = entry.path();
            let Some(ext) = path.extension() else {
                continue;
            };
            if ext != "jsonl" {
                continue;
            }
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if !seen.insert(canonical) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let session_id = stem.to_string();
            sessions.push(build_session_from_file(path, session_id, index.as_deref()));
        }
    }

    sessions.sort_by_key(|s| Reverse(s.started_at));
    Ok(sessions)
}
