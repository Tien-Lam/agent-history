#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    ClaudeCode,
    CopilotCli,
    GeminiCli,
    CodexCli,
    OpenCode,
    Cursor,
    Aider,
    ZedAi,
    Cline,
    ContinueDev,
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderSpec {
    pub provider: Provider,
    pub slug: &'static str,
    pub display_name: &'static str,
    resume: fn(&str) -> String,
}

impl ProviderSpec {
    pub fn resume_command(self, session_id: &str) -> String {
        (self.resume)(session_id)
    }
}

mod resume;

use resume::{
    resume_aider, resume_claude_code, resume_codex_cli, resume_copilot_cli, resume_cursor,
    resume_gemini_cli, resume_opencode, resume_vscode_extension, resume_zed_ai,
};

pub const PROVIDER_SPECS: &[ProviderSpec] = &[
    ProviderSpec {
        provider: Provider::ClaudeCode,
        slug: "claude-code",
        display_name: "Claude Code",
        resume: resume_claude_code,
    },
    ProviderSpec {
        provider: Provider::CopilotCli,
        slug: "copilot-cli",
        display_name: "Copilot CLI",
        resume: resume_copilot_cli,
    },
    ProviderSpec {
        provider: Provider::GeminiCli,
        slug: "gemini-cli",
        display_name: "Gemini CLI",
        resume: resume_gemini_cli,
    },
    ProviderSpec {
        provider: Provider::CodexCli,
        slug: "codex-cli",
        display_name: "Codex CLI",
        resume: resume_codex_cli,
    },
    ProviderSpec {
        provider: Provider::OpenCode,
        slug: "opencode",
        display_name: "OpenCode",
        resume: resume_opencode,
    },
    ProviderSpec {
        provider: Provider::Cursor,
        slug: "cursor",
        display_name: "Cursor",
        resume: resume_cursor,
    },
    ProviderSpec {
        provider: Provider::Aider,
        slug: "aider",
        display_name: "Aider",
        resume: resume_aider,
    },
    ProviderSpec {
        provider: Provider::ZedAi,
        slug: "zed-ai",
        display_name: "Zed AI",
        resume: resume_zed_ai,
    },
    ProviderSpec {
        provider: Provider::Cline,
        slug: "cline",
        display_name: "Cline",
        resume: resume_vscode_extension,
    },
    ProviderSpec {
        provider: Provider::ContinueDev,
        slug: "continue-dev",
        display_name: "Continue.dev",
        resume: resume_vscode_extension,
    },
];

const ALL_PROVIDERS: &[Provider] = &[
    Provider::ClaudeCode,
    Provider::CopilotCli,
    Provider::GeminiCli,
    Provider::CodexCli,
    Provider::OpenCode,
    Provider::Cursor,
    Provider::Aider,
    Provider::ZedAi,
    Provider::Cline,
    Provider::ContinueDev,
];

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
        Self::from_slug(s)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown provider slug {s:?}")))
    }
}

impl Provider {
    pub fn spec(self) -> ProviderSpec {
        PROVIDER_SPECS
            .iter()
            .copied()
            .find(|spec| spec.provider == self)
            .expect("every Provider variant has a ProviderSpec")
    }

    pub fn as_str(self) -> &'static str {
        self.spec().display_name
    }

    /// Stable kebab-case slug used in citation refs, config, and any
    /// other machine-readable context. Must remain stable across releases —
    /// citation refs depend on it for round-tripping.
    pub fn slug(self) -> &'static str {
        self.spec().slug
    }

    /// Inverse of [`Provider::slug`]. Returns `None` for unknown slugs.
    pub fn from_slug(slug: &str) -> Option<Self> {
        PROVIDER_SPECS
            .iter()
            .find(|spec| spec.slug == slug)
            .map(|spec| spec.provider)
    }

    pub fn all() -> &'static [Self] {
        ALL_PROVIDERS
    }

    /// Returns a CLI command to resume the given session.
    ///
    /// Shell-bearing provider commands quote unsafe session IDs to prevent
    /// command injection.
    pub fn resume_command(self, session_id: &str) -> String {
        self.spec().resume_command(session_id)
    }
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
    fn slug_round_trip_for_all_providers() {
        for &p in Provider::all() {
            assert_eq!(
                Provider::from_slug(p.slug()),
                Some(p),
                "slug round-trip for {p:?}"
            );
        }
    }

    #[test]
    fn provider_specs_cover_every_variant_in_order() {
        let from_specs: Vec<Provider> = PROVIDER_SPECS.iter().map(|spec| spec.provider).collect();
        assert_eq!(from_specs, Provider::all());
        for spec in PROVIDER_SPECS {
            assert_eq!(Provider::from_slug(spec.slug), Some(spec.provider));
            assert_eq!(spec.provider.slug(), spec.slug);
            assert_eq!(spec.provider.as_str(), spec.display_name);
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
        assert_eq!(Provider::Aider.slug(), "aider");
        assert_eq!(Provider::ZedAi.slug(), "zed-ai");
        assert_eq!(Provider::Cline.slug(), "cline");
        assert_eq!(Provider::ContinueDev.slug(), "continue-dev");
    }

    #[test]
    fn from_slug_rejects_unknown() {
        assert_eq!(Provider::from_slug(""), None);
        assert_eq!(Provider::from_slug("Claude Code"), None);
        assert_eq!(Provider::from_slug("CLAUDE-CODE"), None);
        assert_eq!(Provider::from_slug("not-a-provider"), None);
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
