use std::path::{Path, PathBuf};

use crate::model::Provider;

use super::aider::AiderProvider;
use super::claude_code::ClaudeCodeProvider;
use super::cline::ClineProvider;
use super::codex_cli::CodexCliProvider;
use super::continue_dev::ContinueDevProvider;
use super::copilot_cli::CopilotCliProvider;
use super::cursor::CursorProvider;
use super::gemini_cli::GeminiCliProvider;
use super::opencode::OpenCodeProvider;
use super::zed_ai::ZedAiProvider;
use super::HistoryProvider;

pub struct RuntimeProviderSpec {
    pub provider: Provider,
    detect: fn() -> Option<Box<dyn HistoryProvider>>,
    from_dirs: fn(Vec<PathBuf>) -> Box<dyn HistoryProvider>,
    remote_candidates: fn(&Path) -> Vec<PathBuf>,
}

impl RuntimeProviderSpec {
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

pub const RUNTIME_PROVIDER_SPECS: &[RuntimeProviderSpec] = &[
    RuntimeProviderSpec {
        provider: Provider::ClaudeCode,
        detect: detect_claude_code,
        from_dirs: claude_code_from_dirs,
        remote_candidates: claude_code_remote_candidates,
    },
    RuntimeProviderSpec {
        provider: Provider::CopilotCli,
        detect: detect_copilot_cli,
        from_dirs: copilot_cli_from_dirs,
        remote_candidates: copilot_cli_remote_candidates,
    },
    RuntimeProviderSpec {
        provider: Provider::GeminiCli,
        detect: detect_gemini_cli,
        from_dirs: gemini_cli_from_dirs,
        remote_candidates: gemini_cli_remote_candidates,
    },
    RuntimeProviderSpec {
        provider: Provider::CodexCli,
        detect: detect_codex_cli,
        from_dirs: codex_cli_from_dirs,
        remote_candidates: codex_cli_remote_candidates,
    },
    RuntimeProviderSpec {
        provider: Provider::OpenCode,
        detect: detect_opencode,
        from_dirs: opencode_from_dirs,
        remote_candidates: opencode_remote_candidates,
    },
    RuntimeProviderSpec {
        provider: Provider::Cursor,
        detect: detect_cursor,
        from_dirs: cursor_from_dirs,
        remote_candidates: cursor_remote_candidates,
    },
    RuntimeProviderSpec {
        provider: Provider::Aider,
        detect: detect_aider,
        from_dirs: aider_from_dirs,
        remote_candidates: aider_remote_candidates,
    },
    RuntimeProviderSpec {
        provider: Provider::ZedAi,
        detect: detect_zed_ai,
        from_dirs: zed_ai_from_dirs,
        remote_candidates: zed_ai_remote_candidates,
    },
    RuntimeProviderSpec {
        provider: Provider::Cline,
        detect: detect_cline,
        from_dirs: cline_from_dirs,
        remote_candidates: cline_remote_candidates,
    },
    RuntimeProviderSpec {
        provider: Provider::ContinueDev,
        detect: detect_continue_dev,
        from_dirs: continue_dev_from_dirs,
        remote_candidates: continue_dev_remote_candidates,
    },
];

pub fn runtime_spec(provider: Provider) -> Option<&'static RuntimeProviderSpec> {
    RUNTIME_PROVIDER_SPECS
        .iter()
        .find(|spec| spec.provider == provider)
}

pub fn provider_from_dirs(provider: Provider, dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    runtime_spec(provider)
        .expect("every Provider has a RuntimeProviderSpec")
        .from_dirs(dirs)
}

/// Candidate base dirs for a provider rooted at a federated source cache.
///
/// The cache may contain either a full home directory or an exact provider
/// history directory. Each provider gets both forms where applicable.
pub fn remote_candidate_dirs(provider: Provider, root: &Path) -> Vec<PathBuf> {
    runtime_spec(provider)
        .expect("every Provider has a RuntimeProviderSpec")
        .remote_candidate_dirs(root)
}

fn claude_code_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(ClaudeCodeProvider::new(dirs))
}

fn copilot_cli_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(CopilotCliProvider::new(dirs))
}

fn gemini_cli_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(GeminiCliProvider::new(dirs))
}

fn codex_cli_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(CodexCliProvider::new(dirs))
}

fn opencode_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(OpenCodeProvider::new(dirs))
}

fn cursor_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(CursorProvider::new(dirs))
}

fn aider_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(AiderProvider::new(dirs))
}

fn zed_ai_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(ZedAiProvider::new(dirs))
}

fn cline_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(ClineProvider::new(dirs))
}

fn continue_dev_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(ContinueDevProvider::new(dirs))
}

fn claude_code_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![root.to_path_buf(), root.join(".claude")]
}

fn copilot_cli_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![
        root.to_path_buf(),
        root.join(".copilot").join("session-state"),
    ]
}

fn gemini_cli_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![root.to_path_buf(), root.join(".gemini")]
}

fn codex_cli_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![root.to_path_buf(), root.join(".codex").join("sessions")]
}

fn opencode_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![
        root.to_path_buf(),
        root.join(".local")
            .join("share")
            .join("opencode")
            .join("storage"),
    ]
}

fn cursor_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![
        root.to_path_buf(),
        root.join(".config").join("Cursor"),
        root.join("Library")
            .join("Application Support")
            .join("Cursor"),
        root.join("AppData").join("Roaming").join("Cursor"),
    ]
}

fn aider_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![root.to_path_buf(), root.join("projects")]
}

fn zed_ai_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![
        root.to_path_buf(),
        root.join(".local").join("share").join("zed"),
        root.join(".config").join("zed"),
        root.join("Library").join("Application Support").join("Zed"),
        root.join("AppData").join("Roaming").join("Zed"),
    ]
}

fn cline_remote_candidates(root: &Path) -> Vec<PathBuf> {
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

fn continue_dev_remote_candidates(root: &Path) -> Vec<PathBuf> {
    vec![root.to_path_buf(), root.join(".continue")]
}

fn detect_claude_code() -> Option<Box<dyn HistoryProvider>> {
    ClaudeCodeProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

fn detect_copilot_cli() -> Option<Box<dyn HistoryProvider>> {
    CopilotCliProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

fn detect_gemini_cli() -> Option<Box<dyn HistoryProvider>> {
    GeminiCliProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

fn detect_codex_cli() -> Option<Box<dyn HistoryProvider>> {
    CodexCliProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

fn detect_opencode() -> Option<Box<dyn HistoryProvider>> {
    OpenCodeProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

fn detect_cursor() -> Option<Box<dyn HistoryProvider>> {
    CursorProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

fn detect_aider() -> Option<Box<dyn HistoryProvider>> {
    AiderProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

fn detect_zed_ai() -> Option<Box<dyn HistoryProvider>> {
    ZedAiProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

fn detect_cline() -> Option<Box<dyn HistoryProvider>> {
    ClineProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

fn detect_continue_dev() -> Option<Box<dyn HistoryProvider>> {
    ContinueDevProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

#[cfg(test)]
mod tests;
