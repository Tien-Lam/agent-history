use std::path::PathBuf;

use super::super::{discovery_error, entry_is_regular_file, ProviderError};
use super::parse::build_session_from_file;
use crate::model::Session;

pub(super) fn base_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();

    if let Some(home) = super::super::home_dir() {
        // OpenCode commonly stores data at ~/.local/share/opencode/storage/
        // even on Windows, so always check this path.
        let local_share = home
            .join(".local")
            .join("share")
            .join("opencode")
            .join("storage");
        result.push(local_share);
    }

    if !super::super::env_var_is_non_empty("AGHIST_HOME") {
        // Also check platform-native data directories.
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

    if let Some(data_dir) = super::super::env_path("OPENCODE_DATA_DIR") {
        result.push(data_dir);
    }

    result
}

pub(super) fn discover_sessions(dirs: &[PathBuf]) -> Result<Vec<Session>, ProviderError> {
    let mut sessions = Vec::new();

    for base in dirs {
        if !base.exists() {
            continue;
        }

        // Scan session/{projectHash}/*.json.
        let session_dir = base.join("session");
        if !session_dir.exists() {
            continue;
        }

        let project_dirs = std::fs::read_dir(&session_dir).map_err(discovery_error("OpenCode"))?;

        for project_entry in project_dirs {
            let project_entry = project_entry.map_err(discovery_error("OpenCode"))?;
            if !project_entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }

            let files =
                std::fs::read_dir(project_entry.path()).map_err(discovery_error("OpenCode"))?;

            for file_entry in files {
                let file_entry = file_entry.map_err(discovery_error("OpenCode"))?;
                if !entry_is_regular_file(&file_entry) {
                    continue;
                }
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
