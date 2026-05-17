pub(super) use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
pub(super) use ratatui::backend::TestBackend;
pub(super) use ratatui::Terminal;

pub(super) use aghist::app::{App, AppMode};
pub(super) use aghist::config::Config;
pub(super) use aghist::provider::HistoryProvider;

pub(super) use super::common::fixtures;
pub(super) use super::common::helpers::{render_to_text, ScriptedEventSource};

pub(super) fn make_app(providers: Vec<Box<dyn HistoryProvider>>) -> App {
    App::new(providers, Config::default())
}

pub(super) fn make_terminal() -> Terminal<TestBackend> {
    let backend = TestBackend::new(120, 40);
    Terminal::new(backend).unwrap()
}

pub(super) fn claude_providers(fixture: &fixtures::FixtureDir) -> Vec<Box<dyn HistoryProvider>> {
    vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )]
}
