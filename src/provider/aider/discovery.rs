use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::super::discovery_error;
use super::ProviderError;

pub(super) const HISTORY_FILE: &str = ".aider.chat.history.md";
const MAX_WALK_DEPTH: usize = 4;

pub(super) fn base_dirs() -> Vec<PathBuf> {
    let mut result = Vec::new();

    if let Some(roots) = std::env::var_os("AIDER_ROOT") {
        result.extend(aider_roots_from_env_value(&roots));
    }

    if let Some(home) = super::super::home_dir() {
        // Spec: "walk ~/projects/". This is also where AGHIST_HOME tests
        // place fixture project trees.
        let projects = home.join("projects");
        if !result.iter().any(|p| p == &projects) {
            result.push(projects);
        }
    }

    result
}

pub(super) fn aider_roots_from_env_value(roots: &OsStr) -> impl Iterator<Item = PathBuf> + '_ {
    std::env::split_paths(roots).filter(|root| !path_is_blank(root))
}

fn path_is_blank(path: &Path) -> bool {
    path.as_os_str().is_empty() || path.as_os_str().to_string_lossy().trim().is_empty()
}

/// Recursively scans `dir` for `.aider.chat.history.md` files. Bounded by
/// [`MAX_WALK_DEPTH`] because Aider history lives at project roots: going
/// deeper just wades into `node_modules`, `.git`, etc.
pub(super) fn collect_history_files(
    dir: &Path,
    depth: usize,
    out: &mut Vec<PathBuf>,
) -> Result<(), ProviderError> {
    if depth > MAX_WALK_DEPTH {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(discovery_error("Aider"))?;
    for entry in entries {
        let entry = entry.map_err(discovery_error("Aider"))?;
        let path = entry.path();
        let ft = entry.file_type().map_err(discovery_error("Aider"))?;
        if ft.is_file() {
            if path.file_name().and_then(|n| n.to_str()) == Some(HISTORY_FILE) {
                out.push(path);
            }
            continue;
        }
        if ft.is_dir() {
            // Skip well-known noise dirs to keep the walk cheap. We don't
            // bother filtering by `.gitignore` - that's overkill for a TUI.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if matches!(
                    name,
                    "node_modules" | ".git" | "target" | "dist" | "build" | ".venv" | "venv"
                ) {
                    continue;
                }
            }
            collect_history_files(&path, depth + 1, out)?;
        }
    }
    Ok(())
}
