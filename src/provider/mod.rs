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
mod load;
pub mod opencode;
mod parse_common;
mod paths;
pub mod registry;
mod text_blocks;
pub mod zed_ai;

pub use error::ProviderError;
pub use load::{
    detect_all_providers, load_messages_for_session, HistoryProvider, ProviderMessageLoad,
    ProviderParseStats,
};

pub(crate) use paths::{discovery_error, home_dir, project_name_from_path};
