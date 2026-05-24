pub(super) fn resume_claude_code(session_id: &str) -> String {
    format!("claude --resume {}", shell_escape(session_id))
}

pub(super) fn resume_copilot_cli(session_id: &str) -> String {
    format!("copilot --resume={}", shell_escape(session_id))
}

pub(super) fn resume_gemini_cli(session_id: &str) -> String {
    format!("gemini --resume {}", shell_escape(session_id))
}

pub(super) fn resume_codex_cli(session_id: &str) -> String {
    let id = codex_resume_id(session_id);
    format!("codex resume {}", shell_escape(id))
}

pub(super) fn resume_opencode(session_id: &str) -> String {
    format!("opencode --session {}", shell_escape(session_id))
}

pub(super) fn resume_aider(_: &str) -> String {
    "aider".to_string()
}

pub(super) fn resume_cursor(_: &str) -> String {
    "cursor".to_string()
}

pub(super) fn resume_zed_ai(_: &str) -> String {
    "zed".to_string()
}

pub(super) fn resume_vscode_extension(_: &str) -> String {
    "code".to_string()
}

/// Wraps a value in single quotes for safe shell interpolation.
/// Single quotes inside the value are escaped as `'\''`.
fn shell_escape(s: &str) -> String {
    if s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Extracts the UUID from a Codex rollout filename stem.
///
/// Rollout files are named `rollout-YYYY-MM-DDThh-mm-ss-<uuid>.jsonl`.
/// The `codex resume` command expects a bare UUID or thread name, not
/// the full filename stem. If a trailing UUID is found, return it;
/// otherwise strip the `rollout-` prefix as a best-effort fallback.
fn codex_resume_id(session_id: &str) -> &str {
    if let Some(tail) = session_id.get(session_id.len().saturating_sub(36)..) {
        if is_uuid_like(tail) {
            return tail;
        }
    }
    session_id.strip_prefix("rollout-").unwrap_or(session_id)
}

fn is_uuid_like(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(idx, byte)| match idx {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::super::Provider;
    use super::*;

    #[test]
    fn resume_command_claude_code() {
        let cmd = Provider::ClaudeCode.resume_command("abc-123-def");
        assert_eq!(cmd, "claude --resume abc-123-def");
    }

    #[test]
    fn resume_command_copilot_cli() {
        let cmd = Provider::CopilotCli.resume_command("ses-id-456");
        assert_eq!(cmd, "copilot --resume=ses-id-456");
    }

    #[test]
    fn resume_command_gemini_cli() {
        let cmd = Provider::GeminiCli.resume_command("uuid-789");
        assert_eq!(cmd, "gemini --resume uuid-789");
    }

    #[test]
    fn resume_command_opencode() {
        let cmd = Provider::OpenCode.resume_command("ses_abc123");
        assert_eq!(cmd, "opencode --session ses_abc123");
    }

    #[test]
    fn resume_command_codex_extracts_uuid() {
        let stem = "rollout-2024-03-15T10-30-00-a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let cmd = Provider::CodexCli.resume_command(stem);
        assert_eq!(cmd, "codex resume a1b2c3d4-e5f6-7890-abcd-ef1234567890");
    }

    #[test]
    fn resume_command_codex_ignores_non_hex_uuid_shape() {
        let stem = "rollout-2024-03-15T10-30-00-z1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let cmd = Provider::CodexCli.resume_command(stem);
        assert_eq!(
            cmd,
            "codex resume 2024-03-15T10-30-00-z1b2c3d4-e5f6-7890-abcd-ef1234567890"
        );
    }

    #[test]
    fn resume_command_codex_strips_prefix_fallback() {
        let cmd = Provider::CodexCli.resume_command("rollout-test123");
        assert_eq!(cmd, "codex resume test123");
    }

    #[test]
    fn resume_command_codex_plain_id() {
        let cmd = Provider::CodexCli.resume_command("my-thread");
        assert_eq!(cmd, "codex resume my-thread");
    }

    #[test]
    fn resume_command_escapes_shell_metacharacters() {
        let cmd = Provider::ClaudeCode.resume_command("abc; rm -rf /");
        assert_eq!(cmd, "claude --resume 'abc; rm -rf /'");
    }

    #[test]
    fn resume_command_cursor() {
        let cmd = Provider::Cursor.resume_command("composer-abc");
        assert_eq!(cmd, "cursor");
    }

    #[test]
    fn resume_command_aider() {
        let cmd = Provider::Aider.resume_command("session-abc");
        assert_eq!(cmd, "aider");
    }

    #[test]
    fn resume_command_zed_ai() {
        let cmd = Provider::ZedAi.resume_command("conversation-abc");
        assert_eq!(cmd, "zed");
    }

    #[test]
    fn resume_command_cline() {
        let cmd = Provider::Cline.resume_command("1698765432000");
        assert_eq!(cmd, "code");
    }

    #[test]
    fn resume_command_continue_dev() {
        let cmd = Provider::ContinueDev.resume_command("abc-uuid");
        assert_eq!(cmd, "code");
    }

    #[test]
    fn shell_escape_safe_id_unquoted() {
        assert_eq!(shell_escape("abc-123_def.txt"), "abc-123_def.txt");
    }

    #[test]
    fn shell_escape_single_quotes() {
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }
}
