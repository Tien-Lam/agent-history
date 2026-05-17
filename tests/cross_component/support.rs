pub(super) use std::num::NonZeroUsize;
pub(super) use std::{fs, thread, time::Duration};

pub(super) use lru::LruCache;

pub(super) use aghist::model::{ContentBlock, Message, Provider, Role};
pub(super) use aghist::provider::claude_code::ClaudeCodeProvider;
pub(super) use aghist::provider::codex_cli::CodexCliProvider;
pub(super) use aghist::provider::copilot_cli::CopilotCliProvider;
pub(super) use aghist::provider::gemini_cli::GeminiCliProvider;
pub(super) use aghist::provider::opencode::OpenCodeProvider;
pub(super) use aghist::provider::HistoryProvider;
pub(super) use aghist::search::SearchIndex;

pub(super) use super::common::fixtures;
pub(super) use super::common::helpers::{copy_dir_recursive, fixtures_dir};

pub(super) fn all_providers() -> Vec<Box<dyn HistoryProvider>> {
    vec![
        Box::new(ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")])),
        Box::new(CopilotCliProvider::new(
            vec![fixtures_dir().join("copilot")],
        )),
        Box::new(GeminiCliProvider::new(vec![fixtures_dir().join("gemini")])),
        Box::new(CodexCliProvider::new(vec![fixtures_dir().join("codex")])),
        Box::new(OpenCodeProvider::new(vec![fixtures_dir().join("opencode")])),
    ]
}
