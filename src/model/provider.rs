#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    ClaudeCode,
    CopilotCli,
    GeminiCli,
    CodexCli,
    OpenCode,
    Cursor,
    ZedAi,
    Cline,
}

/// Serializes as the kebab-case [`Provider::slug`]. This matches the
/// CLI input contract (`--provider claude-code`) and citation refs, so
/// JSON output round-trips through `--provider` without conversion.
impl serde::Serialize for Provider {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.slug())
    }
}

/// Deserializes from the kebab-case [`Provider::slug`]. Unknown slugs
/// (including the legacy `snake_case` forms) are rejected.
impl<'de> serde::Deserialize<'de> for Provider {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = <&str as serde::Deserialize>::deserialize(de)?;
        Self::from_slug(s).ok_or_else(|| {
            serde::de::Error::custom(format!("unknown provider slug {s:?}"))
        })
    }
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::CopilotCli => "Copilot CLI",
            Self::GeminiCli => "Gemini CLI",
            Self::CodexCli => "Codex CLI",
            Self::OpenCode => "OpenCode",
            Self::Cursor => "Cursor",
            Self::ZedAi => "Zed AI",
            Self::Cline => "Cline",
        }
    }

    /// Stable kebab-case slug used in citation refs, config, and any
    /// other machine-readable context. Must remain stable across releases —
    /// citation refs depend on it for round-tripping.
    pub fn slug(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::CopilotCli => "copilot-cli",
            Self::GeminiCli => "gemini-cli",
            Self::CodexCli => "codex-cli",
            Self::OpenCode => "opencode",
            Self::Cursor => "cursor",
            Self::ZedAi => "zed-ai",
            Self::Cline => "cline",
        }
    }

    /// Inverse of [`Provider::slug`]. Returns `None` for unknown slugs.
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "claude-code" => Some(Self::ClaudeCode),
            "copilot-cli" => Some(Self::CopilotCli),
            "gemini-cli" => Some(Self::GeminiCli),
            "codex-cli" => Some(Self::CodexCli),
            "opencode" => Some(Self::OpenCode),
            "cursor" => Some(Self::Cursor),
            "zed-ai" => Some(Self::ZedAi),
            "cline" => Some(Self::Cline),
            _ => None,
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::ClaudeCode,
            Self::CopilotCli,
            Self::GeminiCli,
            Self::CodexCli,
            Self::OpenCode,
            Self::Cursor,
            Self::ZedAi,
            Self::Cline,
        ]
    }

    /// Returns a CLI command to resume the given session.
    ///
    /// The session ID is single-quoted to prevent shell injection.
    pub fn resume_command(self, session_id: &str) -> String {
        let safe_id = shell_escape(session_id);
        match self {
            Self::ClaudeCode => format!("claude --resume {safe_id}"),
            Self::CopilotCli => format!("copilot --resume={safe_id}"),
            Self::GeminiCli => format!("gemini --resume {safe_id}"),
            Self::CodexCli => {
                let id = codex_resume_id(session_id);
                let safe = shell_escape(id);
                format!("codex resume {safe}")
            }
            Self::OpenCode => format!("opencode --session {safe_id}"),
            // Cursor is a GUI app without a CLI flag to resume a specific
            // composer session — best we can do is launch the editor.
            Self::Cursor => "cursor".to_string(),
            // Zed is a GUI editor; the assistant panel cannot be opened to a
            // specific conversation from the CLI, so we just launch the app.
            Self::ZedAi => "zed".to_string(),
            // Cline is a VS Code extension; open VS Code and the user can
            // navigate to the task from the Cline panel.
            Self::Cline => "code".to_string(),
        }
    }
}

/// Wraps a value in single quotes for safe shell interpolation.
/// Single quotes inside the value are escaped as `'\''`.
fn shell_escape(s: &str) -> String {
    if s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.') {
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
        if tail.len() == 36 {
            let b = tail.as_bytes();
            if b[8] == b'-' && b[13] == b'-' && b[18] == b'-' && b[23] == b'-' {
                return tail;
            }
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

    #[test]
    fn resume_command_escapes_shell_metacharacters() {
        let cmd = Provider::ClaudeCode.resume_command("abc; rm -rf /");
        assert_eq!(cmd, "claude --resume 'abc; rm -rf /'");
    }

    #[test]
    fn slug_round_trip_for_all_providers() {
        for &p in Provider::all() {
            assert_eq!(Provider::from_slug(p.slug()), Some(p), "slug round-trip for {p:?}");
        }
    }

    #[test]
    fn slug_values_are_stable_kebab_case() {
        assert_eq!(Provider::ClaudeCode.slug(), "claude-code");
        assert_eq!(Provider::CopilotCli.slug(), "copilot-cli");
        assert_eq!(Provider::GeminiCli.slug(), "gemini-cli");
        assert_eq!(Provider::CodexCli.slug(), "codex-cli");
        assert_eq!(Provider::OpenCode.slug(), "opencode");
        assert_eq!(Provider::Cursor.slug(), "cursor");
        assert_eq!(Provider::ZedAi.slug(), "zed-ai");
        assert_eq!(Provider::Cline.slug(), "cline");
    }

    #[test]
    fn resume_command_cursor() {
        let cmd = Provider::Cursor.resume_command("composer-abc");
        assert_eq!(cmd, "cursor");
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
    fn from_slug_rejects_unknown() {
        assert_eq!(Provider::from_slug(""), None);
        assert_eq!(Provider::from_slug("Claude Code"), None);
        assert_eq!(Provider::from_slug("CLAUDE-CODE"), None);
        assert_eq!(Provider::from_slug("not-a-provider"), None);
    }

    #[test]
    fn shell_escape_safe_id_unquoted() {
        assert_eq!(shell_escape("abc-123_def.txt"), "abc-123_def.txt");
    }

    #[test]
    fn shell_escape_single_quotes() {
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn serialize_uses_kebab_case_slug() {
        for &p in Provider::all() {
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(json, format!("\"{}\"", p.slug()));
        }
    }

    #[test]
    fn deserialize_round_trips_via_slug() {
        for &p in Provider::all() {
            let json = serde_json::to_string(&p).unwrap();
            let back: Provider = serde_json::from_str(&json).unwrap();
            assert_eq!(back, p, "round-trip for {p:?}");
        }
    }

    #[test]
    fn deserialize_rejects_legacy_snake_case() {
        // Pre-fix output emitted "claude_code"; reject it so callers can't
        // silently accept ambiguous slugs.
        assert!(serde_json::from_str::<Provider>("\"claude_code\"").is_err());
        assert!(serde_json::from_str::<Provider>("\"codex_cli\"").is_err());
    }
}
