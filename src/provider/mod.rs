pub mod claude_code;
pub mod codex_cli;
pub mod copilot_cli;
pub mod error;
pub mod gemini_cli;
pub mod opencode;

use std::path::PathBuf;

use crate::model::{Message, Provider, Session};

pub use error::ProviderError;

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
    providers
}
