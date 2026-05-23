use std::path::{Path, PathBuf};

pub(super) fn claude_code_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![root.to_path_buf(), root.join(".claude")]
}

pub(super) fn copilot_cli_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![
        root.to_path_buf(),
        root.join(".copilot").join("session-state"),
    ]
}

pub(super) fn gemini_cli_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![root.to_path_buf(), root.join(".gemini")]
}

pub(super) fn codex_cli_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![root.to_path_buf(), root.join(".codex").join("sessions")]
}

pub(super) fn opencode_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![
        root.to_path_buf(),
        root.join(".local")
            .join("share")
            .join("opencode")
            .join("storage"),
    ]
}

pub(super) fn cursor_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![
        root.to_path_buf(),
        root.join(".config").join("Cursor"),
        root.join("Library")
            .join("Application Support")
            .join("Cursor"),
        root.join("AppData").join("Roaming").join("Cursor"),
    ]
}

pub(super) fn aider_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![root.to_path_buf(), root.join("projects")]
}

pub(super) fn zed_ai_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![
        root.to_path_buf(),
        root.join(".local").join("share").join("zed"),
        root.join(".config").join("zed"),
        root.join("Library").join("Application Support").join("Zed"),
        root.join("AppData").join("Roaming").join("Zed"),
    ]
}

pub(super) fn cline_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![
        root.to_path_buf(),
        root.join(".config")
            .join("Code")
            .join("User")
            .join("globalStorage"),
        root.join(".config")
            .join("Cursor")
            .join("User")
            .join("globalStorage"),
        root.join(".config")
            .join("Windsurf")
            .join("User")
            .join("globalStorage"),
        root.join("Library")
            .join("Application Support")
            .join("Code")
            .join("User")
            .join("globalStorage"),
        root.join("AppData")
            .join("Roaming")
            .join("Code")
            .join("User")
            .join("globalStorage"),
    ]
}

pub(super) fn continue_dev_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![root.to_path_buf(), root.join(".continue")]
}
