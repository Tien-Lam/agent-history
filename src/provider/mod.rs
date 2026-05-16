pub mod aider;
mod aider_parse;
pub mod claude_code;
mod claude_code_parse;
pub mod cline;
mod cline_parse;
pub mod codex_cli;
pub mod continue_dev;
pub mod copilot_cli;
mod copilot_cli_parse;
pub mod cursor;
mod cursor_format;
mod cursor_message;
mod cursor_store;
pub mod error;
pub mod gemini_cli;
pub mod opencode;
mod opencode_parse;
pub mod registry;
mod text_blocks;
pub mod zed_ai;
mod zed_ai_parse;

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
    registry::RUNTIME_PROVIDER_SPECS
        .iter()
        .filter_map(registry::RuntimeProviderSpec::detect)
        .collect()
}

/// Load messages for a discovered session. Prefer an already-constructed
/// provider when available, but fall back to a stateless provider instance so
/// remote/federated sessions can still be loaded even when the same provider
/// is not detected locally.
pub fn load_messages_for_session(
    session: &Session,
    providers: &[Box<dyn HistoryProvider>],
) -> Result<Vec<Message>, ProviderError> {
    if let Some(provider) = providers.iter().find(|p| p.provider() == session.provider) {
        return provider.load_messages(session);
    }

    registry::runtime_spec(session.provider)
        .expect("every Provider variant has a runtime provider spec")
        .stateless()
        .load_messages(session)
}
