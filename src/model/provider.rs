#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    ClaudeCode,
    CopilotCli,
    GeminiCli,
    CodexCli,
    OpenCode,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::CopilotCli => "Copilot CLI",
            Self::GeminiCli => "Gemini CLI",
            Self::CodexCli => "Codex CLI",
            Self::OpenCode => "OpenCode",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::ClaudeCode,
            Self::CopilotCli,
            Self::GeminiCli,
            Self::CodexCli,
            Self::OpenCode,
        ]
    }

    /// Returns a CLI command to resume the given session.
    pub fn resume_command(self, session_id: &str) -> String {
        match self {
            Self::ClaudeCode => format!("claude --resume {session_id}"),
            Self::CopilotCli => format!("copilot --resume={session_id}"),
            Self::GeminiCli => format!("gemini --resume {session_id}"),
            Self::CodexCli => {
                let id = codex_resume_id(session_id);
                format!("codex resume {id}")
            }
            Self::OpenCode => format!("opencode --session {session_id}"),
        }
    }
}

/// Extracts the UUID from a Codex rollout filename stem.
///
/// Rollout files are named `rollout-YYYY-MM-DDThh-mm-ss-<uuid>.jsonl`.
/// The `codex resume` command expects a bare UUID or thread name, not
/// the full filename stem. If a trailing UUID is found, return it;
/// otherwise strip the `rollout-` prefix as a best-effort fallback.
fn codex_resume_id(session_id: &str) -> &str {
    if session_id.len() >= 36 {
        let tail = &session_id[session_id.len() - 36..];
        let b = tail.as_bytes();
        if b[8] == b'-' && b[13] == b'-' && b[18] == b'-' && b[23] == b'-' {
            return tail;
        }
    }
    session_id.strip_prefix("rollout-").unwrap_or(session_id)
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
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
    fn resume_command_codex_strips_prefix_fallback() {
        let cmd = Provider::CodexCli.resume_command("rollout-test123");
        assert_eq!(cmd, "codex resume test123");
    }

    #[test]
    fn resume_command_codex_plain_id() {
        let cmd = Provider::CodexCli.resume_command("my-thread");
        assert_eq!(cmd, "codex resume my-thread");
    }
}
