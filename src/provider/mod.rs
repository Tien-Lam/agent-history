pub mod claude_code;
pub mod cline;
pub mod codex_cli;
pub mod continue_dev;
pub mod copilot_cli;
pub mod cursor;
pub mod error;
pub mod gemini_cli;
pub mod opencode;
pub mod zed_ai;

use std::path::PathBuf;

use crate::model::{Message, Provider, Session};

pub use error::ProviderError;

/// Extracts the final path component from a path string, treating both `/`
/// and `\` as separators. Session files often record cross-platform paths
/// (e.g. `C:\Users\me\proj` written on Windows but read on Linux), so we
/// can't rely on `Path::file_name`, which only honours the host separator.
pub(crate) fn project_name_from_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let basename = trimmed.rsplit(['/', '\\']).next()?;
    if basename.is_empty() || basename.ends_with(':') {
        return None;
    }
    Some(basename.to_string())
}

/// Returns the home directory, respecting `AGHIST_HOME` env var override.
/// When `AGHIST_HOME` is set, it is used instead of the system home directory.
pub(crate) fn home_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("AGHIST_HOME") {
        return Some(PathBuf::from(home));
    }
    directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
}

pub trait HistoryProvider: Send + Sync {
    fn provider(&self) -> Provider;
    fn base_dirs(&self) -> &[PathBuf];
    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError>;
    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError>;
}

pub fn detect_all_providers() -> Vec<Box<dyn HistoryProvider>> {
    let mut providers: Vec<Box<dyn HistoryProvider>> = Vec::new();
    if let Some(p) = claude_code::ClaudeCodeProvider::detect() {
        providers.push(Box::new(p));
    }
    if let Some(p) = gemini_cli::GeminiCliProvider::detect() {
        providers.push(Box::new(p));
    }
    if let Some(p) = copilot_cli::CopilotCliProvider::detect() {
        providers.push(Box::new(p));
    }
    if let Some(p) = codex_cli::CodexCliProvider::detect() {
        providers.push(Box::new(p));
    }
    if let Some(p) = opencode::OpenCodeProvider::detect() {
        providers.push(Box::new(p));
    }
    if let Some(p) = cursor::CursorProvider::detect() {
        providers.push(Box::new(p));
    }
    if let Some(p) = zed_ai::ZedAiProvider::detect() {
        providers.push(Box::new(p));
    }
    if let Some(p) = cline::ClineProvider::detect() {
        providers.push(Box::new(p));
    }
    if let Some(p) = continue_dev::ContinueDevProvider::detect() {
        providers.push(Box::new(p));
    }
    providers
}
