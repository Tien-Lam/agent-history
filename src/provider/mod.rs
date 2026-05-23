pub mod aider;
mod anthropic_content;
pub mod claude_code;
pub mod cline;
pub mod codex_cli;
pub mod continue_dev;
pub mod copilot_cli;
pub mod cursor;
pub mod error;
pub mod gemini_cli;
mod json_text;
pub mod opencode;
mod parse_common;
pub mod registry;
mod text_blocks;
pub mod zed_ai;

use std::path::PathBuf;

use serde::Serialize;

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

pub(crate) fn discovery_error(provider: &'static str) -> impl Fn(std::io::Error) -> ProviderError {
    move |source| ProviderError::Discovery { provider, source }
}

pub trait HistoryProvider: Send + Sync {
    fn provider(&self) -> Provider;
    fn base_dirs(&self) -> &[PathBuf];
    fn discover_sessions(&self) -> Result<Vec<Session>, ProviderError>;
    fn load_messages(&self, session: &Session) -> Result<Vec<Message>, ProviderError>;

    fn load_messages_with_stats(
        &self,
        session: &Session,
    ) -> Result<ProviderMessageLoad, ProviderError> {
        self.load_messages(session)
            .map(ProviderMessageLoad::from_messages)
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ProviderParseStats {
    pub records_seen: usize,
    pub parse_errors: usize,
    pub skipped_records: usize,
    pub empty_content: usize,
}

impl ProviderParseStats {
    pub(crate) fn from_counts(
        records_seen: usize,
        parse_errors: usize,
        skipped_records: usize,
        empty_content: usize,
    ) -> Self {
        Self {
            records_seen,
            parse_errors,
            skipped_records,
            empty_content,
        }
    }

    pub(crate) fn clean_records(records_seen: usize) -> Self {
        Self::from_counts(records_seen, 0, 0, 0)
    }

    pub(crate) fn merge(&mut self, other: &Self) {
        self.records_seen += other.records_seen;
        self.parse_errors += other.parse_errors;
        self.skipped_records += other.skipped_records;
        self.empty_content += other.empty_content;
    }

    pub(crate) fn has_warnings(&self) -> bool {
        self.parse_errors > 0 || self.skipped_records > 0 || self.empty_content > 0
    }

    pub(crate) fn record_seen(&mut self) {
        self.records_seen += 1;
    }

    pub(crate) fn record_parse_error(&mut self) {
        self.parse_errors += 1;
    }

    pub(crate) fn record_skipped_record(&mut self) {
        self.skipped_records += 1;
    }

    pub(crate) fn record_empty_content(&mut self) {
        self.empty_content += 1;
    }
}

#[derive(Debug, Clone)]
pub struct ProviderMessageLoad {
    pub messages: Vec<Message>,
    pub parse_stats: ProviderParseStats,
}

impl ProviderMessageLoad {
    pub fn from_messages(messages: Vec<Message>) -> Self {
        Self {
            messages,
            parse_stats: ProviderParseStats::default(),
        }
    }
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
