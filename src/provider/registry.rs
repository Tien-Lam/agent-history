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
    stateless: fn() -> Box<dyn HistoryProvider>,
}

impl RuntimeProviderSpec {
    pub fn detect(&self) -> Option<Box<dyn HistoryProvider>> {
        (self.detect)()
    }

    pub fn stateless(&self) -> Box<dyn HistoryProvider> {
        (self.stateless)()
    }
}

pub const RUNTIME_PROVIDER_SPECS: &[RuntimeProviderSpec] = &[
    RuntimeProviderSpec {
        provider: Provider::ClaudeCode,
        detect: detect_claude_code,
        stateless: stateless_claude_code,
    },
    RuntimeProviderSpec {
        provider: Provider::CopilotCli,
        detect: detect_copilot_cli,
        stateless: stateless_copilot_cli,
    },
    RuntimeProviderSpec {
        provider: Provider::GeminiCli,
        detect: detect_gemini_cli,
        stateless: stateless_gemini_cli,
    },
    RuntimeProviderSpec {
        provider: Provider::CodexCli,
        detect: detect_codex_cli,
        stateless: stateless_codex_cli,
    },
    RuntimeProviderSpec {
        provider: Provider::OpenCode,
        detect: detect_opencode,
        stateless: stateless_opencode,
    },
    RuntimeProviderSpec {
        provider: Provider::Cursor,
        detect: detect_cursor,
        stateless: stateless_cursor,
    },
    RuntimeProviderSpec {
        provider: Provider::Aider,
        detect: detect_aider,
        stateless: stateless_aider,
    },
    RuntimeProviderSpec {
        provider: Provider::ZedAi,
        detect: detect_zed_ai,
        stateless: stateless_zed_ai,
    },
    RuntimeProviderSpec {
        provider: Provider::Cline,
        detect: detect_cline,
        stateless: stateless_cline,
    },
    RuntimeProviderSpec {
        provider: Provider::ContinueDev,
        detect: detect_continue_dev,
        stateless: stateless_continue_dev,
    },
];

pub fn runtime_spec(provider: Provider) -> Option<&'static RuntimeProviderSpec> {
    RUNTIME_PROVIDER_SPECS
        .iter()
        .find(|spec| spec.provider == provider)
}

pub fn provider_from_dirs(provider: Provider, dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    match provider {
        Provider::ClaudeCode => Box::new(ClaudeCodeProvider::new(dirs)),
        Provider::CopilotCli => Box::new(CopilotCliProvider::new(dirs)),
        Provider::GeminiCli => Box::new(GeminiCliProvider::new(dirs)),
        Provider::CodexCli => Box::new(CodexCliProvider::new(dirs)),
        Provider::OpenCode => Box::new(OpenCodeProvider::new(dirs)),
        Provider::Cursor => Box::new(CursorProvider::new(dirs)),
        Provider::Aider => Box::new(AiderProvider::new(dirs)),
        Provider::ZedAi => Box::new(ZedAiProvider::new(dirs)),
        Provider::Cline => Box::new(ClineProvider::new(dirs)),
        Provider::ContinueDev => Box::new(ContinueDevProvider::new(dirs)),
    }
}

/// Candidate base dirs for a provider rooted at a federated source cache.
///
/// The cache may contain either a full home directory or an exact provider
/// history directory. Each provider gets both forms where applicable.
pub fn remote_candidate_dirs(provider: Provider, root: &Path) -> Vec<PathBuf> {
    match provider {
        Provider::ClaudeCode => vec![root.to_path_buf(), root.join(".claude")],
        Provider::CopilotCli => vec![
            root.to_path_buf(),
            root.join(".copilot").join("session-state"),
        ],
        Provider::GeminiCli => vec![root.to_path_buf(), root.join(".gemini")],
        Provider::CodexCli => vec![root.to_path_buf(), root.join(".codex").join("sessions")],
        Provider::OpenCode => vec![
            root.to_path_buf(),
            root.join(".local")
                .join("share")
                .join("opencode")
                .join("storage"),
        ],
        Provider::Cursor => vec![
            root.to_path_buf(),
            root.join(".config").join("Cursor"),
            root.join("Library")
                .join("Application Support")
                .join("Cursor"),
            root.join("AppData").join("Roaming").join("Cursor"),
        ],
        Provider::Aider => vec![root.to_path_buf(), root.join("projects")],
        Provider::ZedAi => vec![
            root.to_path_buf(),
            root.join(".local").join("share").join("zed"),
            root.join(".config").join("zed"),
            root.join("Library").join("Application Support").join("Zed"),
            root.join("AppData").join("Roaming").join("Zed"),
        ],
        Provider::Cline => vec![
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
        ],
        Provider::ContinueDev => vec![root.to_path_buf(), root.join(".continue")],
    }
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

fn stateless_claude_code() -> Box<dyn HistoryProvider> {
    Box::new(ClaudeCodeProvider::new(Vec::new()))
}

fn stateless_copilot_cli() -> Box<dyn HistoryProvider> {
    Box::new(CopilotCliProvider::new(Vec::new()))
}

fn stateless_gemini_cli() -> Box<dyn HistoryProvider> {
    Box::new(GeminiCliProvider::new(Vec::new()))
}

fn stateless_codex_cli() -> Box<dyn HistoryProvider> {
    Box::new(CodexCliProvider::new(Vec::new()))
}

fn stateless_opencode() -> Box<dyn HistoryProvider> {
    Box::new(OpenCodeProvider::new(Vec::new()))
}

fn stateless_cursor() -> Box<dyn HistoryProvider> {
    Box::new(CursorProvider::new(Vec::new()))
}

fn stateless_aider() -> Box<dyn HistoryProvider> {
    Box::new(AiderProvider::new(Vec::new()))
}

fn stateless_zed_ai() -> Box<dyn HistoryProvider> {
    Box::new(ZedAiProvider::new(Vec::new()))
}

fn stateless_cline() -> Box<dyn HistoryProvider> {
    Box::new(ClineProvider::new(Vec::new()))
}

fn stateless_continue_dev() -> Box<dyn HistoryProvider> {
    Box::new(ContinueDevProvider::new(Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_specs_track_provider_registry_order() {
        let runtime: Vec<Provider> = RUNTIME_PROVIDER_SPECS
            .iter()
            .map(|spec| spec.provider)
            .collect();
        assert_eq!(runtime, Provider::all());
    }

    #[test]
    fn remote_candidate_dirs_cover_every_provider() {
        let root = Path::new("/tmp/aghist-root");
        for &provider in Provider::all() {
            assert!(
                !remote_candidate_dirs(provider, root).is_empty(),
                "missing remote candidate dirs for {provider:?}"
            );
        }
    }
}
