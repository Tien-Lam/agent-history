use std::path::PathBuf;

/// Return the user's home directory, or panic if unavailable.
#[must_use]
pub fn home_dir() -> PathBuf {
    dirs::home_dir().expect("could not determine home directory")
}

/// Return the Claude Code projects directory (`~/.claude/projects/`).
#[must_use]
pub fn claude_projects_dir() -> PathBuf {
    home_dir().join(".claude").join("projects")
}

/// Return the Claude Code sessions directory (`~/.claude/sessions/`).
#[must_use]
pub fn claude_sessions_dir() -> PathBuf {
    home_dir().join(".claude").join("sessions")
}

/// Decode a Claude project directory name back into a filesystem path.
///
/// Claude Code encodes project paths by replacing `/` with `-`.
/// For example, `-home-user-project` becomes `/home/user/project`.
#[must_use]
pub fn decode_project_path(dir_name: &str) -> String {
    if let Some(stripped) = dir_name.strip_prefix('-') {
        // Unix-style path: leading `-` is `/`, remaining `-` are `/`
        format!("/{}", stripped.replace('-', "/"))
    } else {
        // Windows or other: just replace `-` with path separators
        dir_name.replace('-', "/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_unix_path() {
        assert_eq!(
            decode_project_path("-home-user-agent-history"),
            "/home/user/agent/history"
        );
    }

    #[test]
    fn decode_root_path() {
        assert_eq!(decode_project_path("-home"), "/home");
    }

    #[test]
    fn decode_windows_style_path() {
        assert_eq!(
            decode_project_path("C-Users-me-project"),
            "C/Users/me/project"
        );
    }
}
