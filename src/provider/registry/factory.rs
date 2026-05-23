use std::path::PathBuf;

use super::super::aider::AiderProvider;
use super::super::claude_code::ClaudeCodeProvider;
use super::super::cline::ClineProvider;
use super::super::codex_cli::CodexCliProvider;
use super::super::continue_dev::ContinueDevProvider;
use super::super::copilot_cli::CopilotCliProvider;
use super::super::cursor::CursorProvider;
use super::super::gemini_cli::GeminiCliProvider;
use super::super::opencode::OpenCodeProvider;
use super::super::zed_ai::ZedAiProvider;
use super::super::HistoryProvider;

pub(super) fn claude_code_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(ClaudeCodeProvider::new(dirs))
}

pub(super) fn copilot_cli_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(CopilotCliProvider::new(dirs))
}

pub(super) fn gemini_cli_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(GeminiCliProvider::new(dirs))
}

pub(super) fn codex_cli_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(CodexCliProvider::new(dirs))
}

pub(super) fn opencode_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(OpenCodeProvider::new(dirs))
}

pub(super) fn cursor_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(CursorProvider::new(dirs))
}

pub(super) fn aider_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(AiderProvider::new(dirs))
}

pub(super) fn zed_ai_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(ZedAiProvider::new(dirs))
}

pub(super) fn cline_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(ClineProvider::new(dirs))
}

pub(super) fn continue_dev_from_dirs(dirs: Vec<PathBuf>) -> Box<dyn HistoryProvider> {
    Box::new(ContinueDevProvider::new(dirs))
}

pub(super) fn detect_claude_code() -> Option<Box<dyn HistoryProvider>> {
    ClaudeCodeProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

pub(super) fn detect_copilot_cli() -> Option<Box<dyn HistoryProvider>> {
    CopilotCliProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

pub(super) fn detect_gemini_cli() -> Option<Box<dyn HistoryProvider>> {
    GeminiCliProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

pub(super) fn detect_codex_cli() -> Option<Box<dyn HistoryProvider>> {
    CodexCliProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

pub(super) fn detect_opencode() -> Option<Box<dyn HistoryProvider>> {
    OpenCodeProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

pub(super) fn detect_cursor() -> Option<Box<dyn HistoryProvider>> {
    CursorProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

pub(super) fn detect_aider() -> Option<Box<dyn HistoryProvider>> {
    AiderProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

pub(super) fn detect_zed_ai() -> Option<Box<dyn HistoryProvider>> {
    ZedAiProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

pub(super) fn detect_cline() -> Option<Box<dyn HistoryProvider>> {
    ClineProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}

pub(super) fn detect_continue_dev() -> Option<Box<dyn HistoryProvider>> {
    ContinueDevProvider::detect().map(|p| Box::new(p) as Box<dyn HistoryProvider>)
}
