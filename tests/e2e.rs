mod common;

use std::path::PathBuf;

use crossterm::event::KeyCode;
use ratatui::Terminal;

use aghist::app::{App, AppMode};
use aghist::config::Config;
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;

use common::helpers::{
    all_providers, edge_cases_dir, make_terminal, render_to_text, ScriptedEventSource,
};

#[path = "e2e/key_mapping.rs"]
mod key_mapping;
#[path = "e2e/resilience.rs"]
mod resilience;
#[path = "e2e/resume.rs"]
mod resume;
#[path = "e2e/search_debounce.rs"]
mod search_debounce;

fn wide_terminal() -> Terminal<ratatui::backend::TestBackend> {
    let backend = ratatui::backend::TestBackend::new(250, 40);
    Terminal::new(backend).unwrap()
}
