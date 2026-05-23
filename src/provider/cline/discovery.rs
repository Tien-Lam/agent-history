use std::cmp::Reverse;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::super::{discovery_error, ProviderError};
use super::parse::parse_task_dir;
use crate::model::Session;

pub(super) const EXTENSION_ID: &str = "saoudrizwan.claude-dev";
pub(super) const TASKS_SUBDIR: &str = "tasks";

pub(super) fn tasks_dir(base: &Path) -> PathBuf {
    base.join(EXTENSION_ID).join(TASKS_SUBDIR)
}

/// VS Code (and fork) global-storage directories, each being the parent that
/// holds `saoudrizwan.claude-dev/tasks/`. We check VS Code, Cursor, and
/// Windsurf. Honors `CLINE_HOME` for testability.
pub(super) fn base_dirs() -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = Vec::new();

    if let Ok(cline_home) = std::env::var("CLINE_HOME") {
        result.push(PathBuf::from(cline_home));
        return result;
    }

    if let Some(home) = super::super::home_dir() {
        for editor in &["Code", "Cursor", "Windsurf"] {
            result.push(
                home.join(".config")
                    .join(editor)
                    .join("User")
                    .join("globalStorage"),
            );
            result.push(
                home.join("Library")
                    .join("Application Support")
                    .join(editor)
                    .join("User")
                    .join("globalStorage"),
            );
            result.push(
                home.join("AppData")
                    .join("Roaming")
                    .join(editor)
                    .join("User")
                    .join("globalStorage"),
            );
        }
    }

    result
}

pub(super) fn discover_sessions(dirs: &[PathBuf]) -> Result<Vec<Session>, ProviderError> {
    let mut sessions = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for base in dirs {
        let td = tasks_dir(base);
        if !td.is_dir() {
            continue;
        }
        let entries = std::fs::read_dir(&td).map_err(discovery_error("Cline"))?;
        for entry in entries {
            let entry = entry.map_err(discovery_error("Cline"))?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if !seen.insert(canonical) {
                continue;
            }
            if let Some(session) = parse_task_dir(&path) {
                sessions.push(session);
            }
        }
    }

    sessions.sort_by_key(|s| Reverse(s.started_at));
    Ok(sessions)
}
