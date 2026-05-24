use std::collections::HashSet;
use std::path::PathBuf;

use super::super::{discovery_error, ProviderError};
use super::parse::read_session;
use crate::model::Session;

pub(super) const CONVERSATIONS_SUBDIR: &str = "conversations";

/// Zed base directories, ordered by likelihood. Each entry is the parent
/// directory that holds `conversations/`. Honors `AGHIST_HOME` (for tests)
/// and `ZED_HOME` as a power-user override.
pub(super) fn base_dirs() -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = Vec::new();

    if let Some(zed_home) = super::super::env_path("ZED_HOME") {
        result.push(zed_home);
    }

    if let Some(home) = super::super::home_dir() {
        // Linux (XDG data): ~/.local/share/zed
        result.push(home.join(".local").join("share").join("zed"));
        // Linux (XDG config): ~/.config/zed - older Zed builds wrote here
        result.push(home.join(".config").join("zed"));
        // macOS: ~/Library/Application Support/Zed
        result.push(home.join("Library").join("Application Support").join("Zed"));
        // Windows: %APPDATA%\Zed (mirrored under home for AGHIST_HOME tests)
        result.push(home.join("AppData").join("Roaming").join("Zed"));
    }

    if !super::super::env_var_is_non_empty("AGHIST_HOME") {
        if let Some(base) = directories::BaseDirs::new() {
            let appdata = base.config_dir().join("Zed");
            if !result.iter().any(|p| p == &appdata) {
                result.push(appdata);
            }
            let data = base.data_dir().join("Zed");
            if !result.iter().any(|p| p == &data) {
                result.push(data);
            }
        }
    }

    result
}

pub(super) fn discover_sessions(dirs: &[PathBuf]) -> Result<Vec<Session>, ProviderError> {
    let mut sessions = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for base in dirs {
        let conv_dir = base.join(CONVERSATIONS_SUBDIR);
        if !conv_dir.exists() {
            continue;
        }
        let entries = std::fs::read_dir(&conv_dir).map_err(discovery_error("Zed AI"))?;
        for entry in entries {
            let entry = entry.map_err(discovery_error("Zed AI"))?;
            let path = entry.path();
            if path.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if !seen.insert(canonical) {
                continue;
            }
            match read_session(&path) {
                Ok(Some(s)) => sessions.push(s),
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping unreadable Zed conversation");
                }
            }
        }
    }

    sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
    Ok(sessions)
}
