use std::path::{Path, PathBuf};

use crate::model::provider::{
    ProviderSpec, AIDER_SPEC, CLAUDE_CODE_SPEC, CLINE_SPEC, CODEX_CLI_SPEC, CONTINUE_DEV_SPEC,
    COPILOT_CLI_SPEC, CURSOR_SPEC, GEMINI_CLI_SPEC, OPENCODE_SPEC, ZED_AI_SPEC,
};
use crate::model::Provider;

use super::HistoryProvider;

mod factory;
mod remote;

use factory::{
    aider_from_dirs, claude_code_from_dirs, cline_from_dirs, codex_cli_from_dirs,
    continue_dev_from_dirs, copilot_cli_from_dirs, cursor_from_dirs, detect_aider,
    detect_claude_code, detect_cline, detect_codex_cli, detect_continue_dev, detect_copilot_cli,
    detect_cursor, detect_gemini_cli, detect_opencode, detect_zed_ai, gemini_cli_from_dirs,
    opencode_from_dirs, zed_ai_from_dirs,
};
use remote::{
    aider_remote_candidates, claude_code_remote_candidates, cline_remote_candidates,
    codex_cli_remote_candidates, continue_dev_remote_candidates, copilot_cli_remote_candidates,
    cursor_remote_candidates, gemini_cli_remote_candidates, opencode_remote_candidates,
    zed_ai_remote_candidates,
};

pub struct RuntimeProviderSpec {
    pub metadata: ProviderSpec,
    detect: fn() -> Option<Box<dyn HistoryProvider>>,
    from_dirs: fn(Vec<PathBuf>) -> Box<dyn HistoryProvider>,
    remote_candidates: fn(&Path) -> Vec<PathBuf>,
}

impl RuntimeProviderSpec {
    pub fn provider(&self) -> Provider {
        self.metadata.provider
    }

    pub fn detect(&self) -> Option<Box<dyn HistoryProvider>> {
        (self.detect)()
    }

    pub fn from_dirs(&self, dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
        (self.from_dirs)(dirs)
    }

    pub fn stateless(&self) -> Box<dyn HistoryProvider> {
        self.from_dirs(Vec::new())
    }

    pub fn remote_candidate_dirs(&self, root: &Path) -> Vec<PathBuf> {
        (self.remote_candidates)(root)
    }
}

pub const CLAUDE_CODE_RUNTIME_SPEC: RuntimeProviderSpec = RuntimeProviderSpec {
    metadata: CLAUDE_CODE_SPEC,
    detect: detect_claude_code,
    from_dirs: claude_code_from_dirs,
    remote_candidates: claude_code_remote_candidates,
};

pub const COPILOT_CLI_RUNTIME_SPEC: RuntimeProviderSpec = RuntimeProviderSpec {
    metadata: COPILOT_CLI_SPEC,
    detect: detect_copilot_cli,
    from_dirs: copilot_cli_from_dirs,
    remote_candidates: copilot_cli_remote_candidates,
};

pub const GEMINI_CLI_RUNTIME_SPEC: RuntimeProviderSpec = RuntimeProviderSpec {
    metadata: GEMINI_CLI_SPEC,
    detect: detect_gemini_cli,
    from_dirs: gemini_cli_from_dirs,
    remote_candidates: gemini_cli_remote_candidates,
};

pub const CODEX_CLI_RUNTIME_SPEC: RuntimeProviderSpec = RuntimeProviderSpec {
    metadata: CODEX_CLI_SPEC,
    detect: detect_codex_cli,
    from_dirs: codex_cli_from_dirs,
    remote_candidates: codex_cli_remote_candidates,
};

pub const OPENCODE_RUNTIME_SPEC: RuntimeProviderSpec = RuntimeProviderSpec {
    metadata: OPENCODE_SPEC,
    detect: detect_opencode,
    from_dirs: opencode_from_dirs,
    remote_candidates: opencode_remote_candidates,
};

pub const CURSOR_RUNTIME_SPEC: RuntimeProviderSpec = RuntimeProviderSpec {
    metadata: CURSOR_SPEC,
    detect: detect_cursor,
    from_dirs: cursor_from_dirs,
    remote_candidates: cursor_remote_candidates,
};

pub const AIDER_RUNTIME_SPEC: RuntimeProviderSpec = RuntimeProviderSpec {
    metadata: AIDER_SPEC,
    detect: detect_aider,
    from_dirs: aider_from_dirs,
    remote_candidates: aider_remote_candidates,
};

pub const ZED_AI_RUNTIME_SPEC: RuntimeProviderSpec = RuntimeProviderSpec {
    metadata: ZED_AI_SPEC,
    detect: detect_zed_ai,
    from_dirs: zed_ai_from_dirs,
    remote_candidates: zed_ai_remote_candidates,
};

pub const CLINE_RUNTIME_SPEC: RuntimeProviderSpec = RuntimeProviderSpec {
    metadata: CLINE_SPEC,
    detect: detect_cline,
    from_dirs: cline_from_dirs,
    remote_candidates: cline_remote_candidates,
};

pub const CONTINUE_DEV_RUNTIME_SPEC: RuntimeProviderSpec = RuntimeProviderSpec {
    metadata: CONTINUE_DEV_SPEC,
    detect: detect_continue_dev,
    from_dirs: continue_dev_from_dirs,
    remote_candidates: continue_dev_remote_candidates,
};

pub const RUNTIME_PROVIDER_SPECS: &[RuntimeProviderSpec] = &[
    CLAUDE_CODE_RUNTIME_SPEC,
    COPILOT_CLI_RUNTIME_SPEC,
    GEMINI_CLI_RUNTIME_SPEC,
    CODEX_CLI_RUNTIME_SPEC,
    OPENCODE_RUNTIME_SPEC,
    CURSOR_RUNTIME_SPEC,
    AIDER_RUNTIME_SPEC,
    ZED_AI_RUNTIME_SPEC,
    CLINE_RUNTIME_SPEC,
    CONTINUE_DEV_RUNTIME_SPEC,
];

pub fn runtime_spec(provider: Provider) -> &'static RuntimeProviderSpec {
    match provider {
        Provider::ClaudeCode => &CLAUDE_CODE_RUNTIME_SPEC,
        Provider::CopilotCli => &COPILOT_CLI_RUNTIME_SPEC,
        Provider::GeminiCli => &GEMINI_CLI_RUNTIME_SPEC,
        Provider::CodexCli => &CODEX_CLI_RUNTIME_SPEC,
        Provider::OpenCode => &OPENCODE_RUNTIME_SPEC,
        Provider::Cursor => &CURSOR_RUNTIME_SPEC,
        Provider::Aider => &AIDER_RUNTIME_SPEC,
        Provider::ZedAi => &ZED_AI_RUNTIME_SPEC,
        Provider::Cline => &CLINE_RUNTIME_SPEC,
        Provider::ContinueDev => &CONTINUE_DEV_RUNTIME_SPEC,
    }
}

pub fn provider_from_dirs(provider: Provider, dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    runtime_spec(provider).from_dirs(dirs)
}

/// Candidate base dirs for a provider rooted at a federated source cache.
///
/// The cache may contain either a full home directory or an exact provider
/// history directory. Each provider gets both forms where applicable.
pub fn remote_candidate_dirs(provider: Provider, root: &Path) -> Vec<PathBuf> {
    runtime_spec(provider).remote_candidate_dirs(root)
}

#[cfg(test)]
mod tests;
