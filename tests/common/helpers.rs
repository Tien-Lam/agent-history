use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;
use std::{fs, path::Path};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use aghist::event::EventSource;
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn edge_cases_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edge_cases")
}

pub fn all_providers() -> Vec<Box<dyn HistoryProvider>> {
    vec![
        Box::new(ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")])),
        Box::new(CopilotCliProvider::new(vec![fixtures_dir().join("copilot")])),
        Box::new(GeminiCliProvider::new(vec![fixtures_dir().join("gemini")])),
        Box::new(CodexCliProvider::new(vec![fixtures_dir().join("codex")])),
        Box::new(OpenCodeProvider::new(vec![fixtures_dir().join("opencode")])),
    ]
}

pub fn make_terminal() -> Terminal<TestBackend> {
    let backend = TestBackend::new(120, 40);
    Terminal::new(backend).unwrap()
}

pub fn render_to_text(terminal: &Terminal<TestBackend>) -> String {
    let buf = terminal.backend().buffer();
    let area = buf.area;
    let mut result = String::new();
    for y in area.y..area.y + area.height {
        let mut line = String::new();
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell((x, y)) {
                line.push_str(cell.symbol());
            }
        }
        result.push_str(line.trim_end());
        result.push('\n');
    }
    result
}

pub fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_recursive(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// A scripted event source that yields pre-recorded events for testing
/// the full event loop via `app.run_with_event_source()`.
pub struct ScriptedEventSource {
    events: VecDeque<Option<Event>>,
}

impl ScriptedEventSource {
    pub fn new(events: Vec<Event>) -> Self {
        Self {
            events: events.into_iter().map(Some).collect(),
        }
    }

    pub fn from_keys(keys: Vec<KeyCode>) -> Self {
        let events = keys
            .into_iter()
            .map(|code| {
                Event::Key(KeyEvent::new_with_kind(
                    code,
                    KeyModifiers::NONE,
                    KeyEventKind::Press,
                ))
            })
            .collect();
        Self::new(events)
    }
}

impl EventSource for ScriptedEventSource {
    fn poll_event(&mut self, _timeout: Duration) -> std::io::Result<Option<Event>> {
        Ok(self.events.pop_front().flatten())
    }
}
