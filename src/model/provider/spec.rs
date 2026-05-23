use super::resume::{
    resume_aider, resume_claude_code, resume_codex_cli, resume_copilot_cli, resume_cursor,
    resume_gemini_cli, resume_opencode, resume_vscode_extension, resume_zed_ai,
};
use super::Provider;

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

pub(super) const ALL_PROVIDERS: &[Provider] = &[
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
