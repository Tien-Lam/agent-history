use std::path::PathBuf;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use aghist::app::{App, AppMode};
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn edge_cases_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edge_cases")
}

fn all_providers() -> Vec<Box<dyn HistoryProvider>> {
    vec![
        Box::new(ClaudeCodeProvider::new(vec![fixtures_dir().join("claude")])),
        Box::new(CopilotCliProvider::new(vec![fixtures_dir().join("copilot")])),
        Box::new(GeminiCliProvider::new(vec![fixtures_dir().join("gemini")])),
        Box::new(CodexCliProvider::new(vec![fixtures_dir().join("codex")])),
        Box::new(OpenCodeProvider::new(vec![fixtures_dir().join("opencode")])),
    ]
}

fn make_terminal() -> Terminal<TestBackend> {
    let backend = TestBackend::new(120, 40);
    Terminal::new(backend).unwrap()
}

// ─── TUI smoke test: construct App, render, verify no panics ─────────────────

#[test]
fn app_initial_render_no_panic() {
    let mut app = App::new(all_providers());
    let mut terminal = make_terminal();

    app.load_sessions();
    terminal.draw(|frame| app.render(frame)).unwrap();

    assert_eq!(app.mode(), AppMode::Browse);
    assert!(!app.is_loading());
    assert_eq!(app.session_count(), 5);
}

#[test]
fn app_navigate_and_select_session() {
    let mut app = App::new(all_providers());
    let mut terminal = make_terminal();

    app.load_sessions();

    // Navigate down
    app.dispatch(aghist::action::Action::NextItem);
    app.dispatch(aghist::action::Action::NextItem);
    terminal.draw(|frame| app.render(frame)).unwrap();
    assert_eq!(app.selected_index(), Some(2));

    // Select session → switches to ViewSession mode
    app.dispatch(aghist::action::Action::SelectSession);
    terminal.draw(|frame| app.render(frame)).unwrap();
    assert_eq!(app.mode(), AppMode::ViewSession);

    // Esc → back to Browse
    app.dispatch(aghist::action::Action::BackToList);
    assert_eq!(app.mode(), AppMode::Browse);
}

#[test]
fn app_scroll_in_session_view() {
    let mut app = App::new(all_providers());
    let mut terminal = make_terminal();

    app.load_sessions();
    app.dispatch(aghist::action::Action::SelectSession);
    assert_eq!(app.mode(), AppMode::ViewSession);

    // Scroll down
    app.dispatch(aghist::action::Action::ScrollDown);
    app.dispatch(aghist::action::Action::ScrollDown);
    app.dispatch(aghist::action::Action::PageDown);
    terminal.draw(|frame| app.render(frame)).unwrap();

    // Scroll up
    app.dispatch(aghist::action::Action::ScrollUp);
    app.dispatch(aghist::action::Action::PageUp);
    terminal.draw(|frame| app.render(frame)).unwrap();

    // Go to top/bottom
    app.dispatch(aghist::action::Action::GoToBottom);
    app.dispatch(aghist::action::Action::GoToTop);
    terminal.draw(|frame| app.render(frame)).unwrap();
}

#[test]
fn app_toggle_tool_calls() {
    let mut app = App::new(all_providers());
    let mut terminal = make_terminal();

    app.load_sessions();
    app.dispatch(aghist::action::Action::SelectSession);
    app.dispatch(aghist::action::Action::ToggleToolCalls);
    terminal.draw(|frame| app.render(frame)).unwrap();

    // Toggle back
    app.dispatch(aghist::action::Action::ToggleToolCalls);
    terminal.draw(|frame| app.render(frame)).unwrap();
}

#[test]
fn app_help_overlay() {
    let mut app = App::new(all_providers());
    let mut terminal = make_terminal();

    app.load_sessions();
    app.dispatch(aghist::action::Action::ToggleHelp);
    assert_eq!(app.mode(), AppMode::Help);
    terminal.draw(|frame| app.render(frame)).unwrap();

    app.dispatch(aghist::action::Action::ToggleHelp);
    assert_eq!(app.mode(), AppMode::Browse);
}

#[test]
fn app_go_to_top_and_bottom() {
    let mut app = App::new(all_providers());
    app.load_sessions();

    app.dispatch(aghist::action::Action::GoToBottom);
    assert_eq!(app.selected_index(), Some(4)); // 5 sessions, 0-indexed

    app.dispatch(aghist::action::Action::GoToTop);
    assert_eq!(app.selected_index(), Some(0));
}

#[test]
fn app_quit_action() {
    let mut app = App::new(all_providers());
    app.load_sessions();

    assert!(!app.should_quit());
    app.dispatch(aghist::action::Action::Quit);
    assert!(app.should_quit());
}

// ─── Key mapping integration ─────────────────────────────────────────────────

#[test]
fn key_mapping_browse_mode() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use aghist::event::map_key_event;

    let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
    let action = map_key_event(key, AppMode::Browse);
    assert!(matches!(action, Some(aghist::action::Action::NextItem)));

    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let action = map_key_event(key, AppMode::Browse);
    assert!(matches!(action, Some(aghist::action::Action::SelectSession)));

    let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let action = map_key_event(key, AppMode::Browse);
    assert!(matches!(action, Some(aghist::action::Action::Quit)));
}

#[test]
fn key_mapping_view_mode() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use aghist::event::map_key_event;

    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let action = map_key_event(key, AppMode::ViewSession);
    assert!(matches!(action, Some(aghist::action::Action::BackToList)));

    let key = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
    let action = map_key_event(key, AppMode::ViewSession);
    assert!(matches!(action, Some(aghist::action::Action::ToggleToolCalls)));
}

// ─── Error resilience E2E ────────────────────────────────────────────────────

#[test]
fn app_with_corrupt_fixtures_no_crash() {
    let providers: Vec<Box<dyn HistoryProvider>> = vec![
        Box::new(ClaudeCodeProvider::new(vec![edge_cases_dir().join("claude")])),
        Box::new(CopilotCliProvider::new(vec![edge_cases_dir().join("copilot")])),
        Box::new(GeminiCliProvider::new(vec![edge_cases_dir().join("gemini")])),
        Box::new(CodexCliProvider::new(vec![edge_cases_dir().join("codex")])),
        Box::new(OpenCodeProvider::new(vec![edge_cases_dir().join("opencode")])),
    ];

    let mut app = App::new(providers);
    let mut terminal = make_terminal();

    // Should not panic despite corrupt fixture data
    app.load_sessions();
    terminal.draw(|frame| app.render(frame)).unwrap();

    // Should have loaded valid sessions only
    assert!(app.session_count() > 0);

    // Navigate and select a session — should not panic
    app.dispatch(aghist::action::Action::SelectSession);
    terminal.draw(|frame| app.render(frame)).unwrap();
}

// ─── Empty state E2E ─────────────────────────────────────────────────────────

#[test]
fn app_with_no_providers_empty_state() {
    let providers: Vec<Box<dyn HistoryProvider>> = Vec::new();
    let mut app = App::new(providers);
    let mut terminal = make_terminal();

    app.load_sessions();
    terminal.draw(|frame| app.render(frame)).unwrap();

    assert_eq!(app.session_count(), 0);
    assert_eq!(app.mode(), AppMode::Browse);
}

#[test]
fn app_with_nonexistent_dirs_empty_state() {
    let fake = PathBuf::from("/nonexistent/path");
    let providers: Vec<Box<dyn HistoryProvider>> = vec![
        Box::new(ClaudeCodeProvider::new(vec![fake.clone()])),
        Box::new(CopilotCliProvider::new(vec![fake.clone()])),
        Box::new(GeminiCliProvider::new(vec![fake.clone()])),
        Box::new(CodexCliProvider::new(vec![fake.clone()])),
        Box::new(OpenCodeProvider::new(vec![fake])),
    ];

    let mut app = App::new(providers);
    let mut terminal = make_terminal();

    app.load_sessions();
    terminal.draw(|frame| app.render(frame)).unwrap();

    assert_eq!(app.session_count(), 0);
}
