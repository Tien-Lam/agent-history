use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{fs, path::Path};

use assert_cmd::Command;
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

static COMMAND_ID: AtomicUsize = AtomicUsize::new(0);

pub fn aghist_bin() -> PathBuf {
    assert_cmd::cargo::cargo_bin("aghist")
}

pub fn aghist_command() -> Command {
    Command::cargo_bin("aghist").unwrap()
}

pub fn isolated_aghist(label: &str) -> Command {
    let id = COMMAND_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("aghist-{label}-{}-{id}", std::process::id()));
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();

    let mut cmd = aghist_command();
    cmd.env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", root.join("config.toml"));
    cmd
}

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn edge_cases_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edge_cases")
}

pub fn all_providers() -> Vec<Box<dyn HistoryProvider>> {
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

pub struct FixtureHome {
    dir: tempfile::TempDir,
}

impl FixtureHome {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn add_claude(&self, fixture: &super::fixtures::core::FixtureDir) {
        copy_dir_recursive(&fixture.base_path, &self.path().join(".claude"));
    }

    pub fn add_codex(&self, fixture: &super::fixtures::core::FixtureDir) {
        copy_dir_recursive(
            &fixture.base_path,
            &self.path().join(".codex").join("sessions"),
        );
    }
}

pub struct RemoteSourceCache {
    pub empty_home: tempfile::TempDir,
    pub _workdir: tempfile::TempDir,
    pub cache_dir: PathBuf,
    pub config_path: PathBuf,
}

pub fn laptop_remote_source(remote_base_path: &Path) -> RemoteSourceCache {
    laptop_remote_source_with_config(remote_base_path, "")
}

pub fn laptop_remote_source_with_config(
    remote_base_path: &Path,
    extra_config: &str,
) -> RemoteSourceCache {
    let empty_home = tempfile::tempdir().unwrap();
    let workdir = tempfile::tempdir().unwrap();
    let cache_dir = workdir.path().join("cache");
    let remote_data = cache_dir.join("laptop").join("data");
    fs::create_dir_all(&remote_data).unwrap();
    copy_dir_recursive(remote_base_path, &remote_data.join(".claude"));

    let config_path = workdir.path().join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"[[sources]]
name = "laptop"
host = "laptop.local"
path = "/home/x/.claude"
transport = "ssh"
{extra_config}"#,
        ),
    )
    .unwrap();

    RemoteSourceCache {
        empty_home,
        _workdir: workdir,
        cache_dir,
        config_path,
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

    /// Insert N empty ticks (no key event) at the current position.
    /// Useful for giving background threads (e.g. search indexer) time to complete.
    pub fn with_idle_ticks(mut self, n: usize) -> Self {
        for _ in 0..n {
            self.events.push_back(None);
        }
        self
    }

    /// Append a key event to the end of the queue.
    pub fn then_key(mut self, code: KeyCode) -> Self {
        self.events
            .push_back(Some(Event::Key(KeyEvent::new_with_kind(
                code,
                KeyModifiers::NONE,
                KeyEventKind::Press,
            ))));
        self
    }
}

impl EventSource for ScriptedEventSource {
    fn poll_event(&mut self, timeout: Duration) -> std::io::Result<Option<Event>> {
        if let Some(evt) = self.events.pop_front() {
            if evt.is_none() {
                // Idle tick: sleep for the poll timeout to give background threads time
                std::thread::sleep(timeout);
            }
            Ok(evt)
        } else {
            // Auto-quit when events are exhausted to prevent infinite loop
            Ok(Some(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ))))
        }
    }
}
