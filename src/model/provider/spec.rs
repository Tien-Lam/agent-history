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

pub const CLAUDE_CODE_SPEC: ProviderSpec = ProviderSpec {
    provider: Provider::ClaudeCode,
    slug: "claude-code",
    display_name: "Claude Code",
    resume: resume_claude_code,
};

pub const COPILOT_CLI_SPEC: ProviderSpec = ProviderSpec {
    provider: Provider::CopilotCli,
    slug: "copilot-cli",
    display_name: "Copilot CLI",
    resume: resume_copilot_cli,
};

pub const GEMINI_CLI_SPEC: ProviderSpec = ProviderSpec {
    provider: Provider::GeminiCli,
    slug: "gemini-cli",
    display_name: "Gemini CLI",
    resume: resume_gemini_cli,
};

pub const CODEX_CLI_SPEC: ProviderSpec = ProviderSpec {
    provider: Provider::CodexCli,
    slug: "codex-cli",
    display_name: "Codex CLI",
    resume: resume_codex_cli,
};

pub const OPENCODE_SPEC: ProviderSpec = ProviderSpec {
    provider: Provider::OpenCode,
    slug: "opencode",
    display_name: "OpenCode",
    resume: resume_opencode,
};

pub const CURSOR_SPEC: ProviderSpec = ProviderSpec {
    provider: Provider::Cursor,
    slug: "cursor",
    display_name: "Cursor",
    resume: resume_cursor,
};

pub const AIDER_SPEC: ProviderSpec = ProviderSpec {
    provider: Provider::Aider,
    slug: "aider",
    display_name: "Aider",
    resume: resume_aider,
};

pub const ZED_AI_SPEC: ProviderSpec = ProviderSpec {
    provider: Provider::ZedAi,
    slug: "zed-ai",
    display_name: "Zed AI",
    resume: resume_zed_ai,
};

pub const CLINE_SPEC: ProviderSpec = ProviderSpec {
    provider: Provider::Cline,
    slug: "cline",
    display_name: "Cline",
    resume: resume_vscode_extension,
};

pub const CONTINUE_DEV_SPEC: ProviderSpec = ProviderSpec {
    provider: Provider::ContinueDev,
    slug: "continue-dev",
    display_name: "Continue.dev",
    resume: resume_vscode_extension,
};

pub const PROVIDER_SPECS: &[ProviderSpec] = &[
    CLAUDE_CODE_SPEC,
    COPILOT_CLI_SPEC,
    GEMINI_CLI_SPEC,
    CODEX_CLI_SPEC,
    OPENCODE_SPEC,
    CURSOR_SPEC,
    AIDER_SPEC,
    ZED_AI_SPEC,
    CLINE_SPEC,
    CONTINUE_DEV_SPEC,
];

pub(super) const ALL_PROVIDERS: &[Provider] = &[
    CLAUDE_CODE_SPEC.provider,
    COPILOT_CLI_SPEC.provider,
    GEMINI_CLI_SPEC.provider,
    CODEX_CLI_SPEC.provider,
    OPENCODE_SPEC.provider,
    CURSOR_SPEC.provider,
    AIDER_SPEC.provider,
    ZED_AI_SPEC.provider,
    CLINE_SPEC.provider,
    CONTINUE_DEV_SPEC.provider,
];
