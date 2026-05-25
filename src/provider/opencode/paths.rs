use std::path::{Path, PathBuf};

pub(crate) fn storage_base_from_source_path(source_path: &Path) -> Option<&Path> {
    if source_path.is_dir() {
        return Some(source_path);
    }

    let project_dir = source_path.parent()?;
    let session_dir = project_dir.parent()?;
    if session_dir.file_name().and_then(|name| name.to_str()) != Some("session") {
        return None;
    }
    session_dir.parent()
}

pub(crate) fn message_dir(storage_base: &Path, session_id: &str) -> PathBuf {
    storage_base.join("message").join(session_id)
}

pub(crate) fn part_root(storage_base: &Path) -> PathBuf {
    storage_base.join("part")
}
