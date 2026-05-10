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

fn wide_terminal() -> Terminal<ratatui::backend::TestBackend> {
    let backend = ratatui::backend::TestBackend::new(250, 40);
    Terminal::new(backend).unwrap()
}

// ─── Resume command via real key press ──────────────────────────────────────

#[test]
fn resume_command_claude_code() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    // First session is Claude Code, press 'y' to copy resume command
    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('y')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("claude --resume session-abc123"),
        "status bar should show Claude resume command, got:\n{text}"
    );
}

#[test]
fn resume_command_copilot_cli() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    // Navigate to index 1 (Copilot), press 'y'
    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('j'),
        KeyCode::Char('y'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("copilot --resume=copilot-session-001"),
        "status bar should show Copilot resume command, got:\n{text}"
    );
}

#[test]
fn resume_command_gemini_cli() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('y'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("gemini --resume gemini-sess-001"),
        "status bar should show Gemini resume command, got:\n{text}"
    );
}

#[test]
fn resume_command_codex_cli() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('G'), // go to bottom (index 4)
        KeyCode::Char('k'), // up to index 3 (Codex)
        KeyCode::Char('y'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("codex resume test123"),
        "status bar should show Codex resume command, got:\n{text}"
    );
}

#[test]
fn resume_command_opencode() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('G'), // go to bottom (index 4 = OpenCode)
        KeyCode::Char('y'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("opencode --session sess-001"),
        "status bar should show OpenCode resume command, got:\n{text}"
    );
}

#[test]
fn resume_command_in_view_mode() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select first session (Claude)
        KeyCode::Char('y'), // resume command from view mode
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::ViewSession);
    let text = render_to_text(&terminal);
    assert!(
        text.contains("claude --resume session-abc123"),
        "resume command should work from view mode, got:\n{text}"
    );
}

#[test]
fn resume_command_no_selection() {
    let providers: Vec<Box<dyn HistoryProvider>> = Vec::new();
    let mut app = App::new(providers, Config::default());
    let mut terminal = wide_terminal();

    // Press 'y' with no sessions — should not show resume command
    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('y')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        !text.contains("--resume"),
        "should not show resume command with no sessions, got:\n{text}"
    );
}

// ─── Key mapping unit tests ────────────────────────────────────────────────

#[test]
fn key_mapping_browse_mode() {
    use crossterm::event::{KeyEvent, KeyModifiers};
    use aghist::event::map_key_event;

    let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Browse, false),
        Some(aghist::action::Action::NextItem)
    ));

    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Browse, false),
        Some(aghist::action::Action::SelectSession)
    ));

    let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Browse, false),
        Some(aghist::action::Action::CopyResumeCommand)
    ));

    let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(matches!(
        map_key_event(key, AppMode::Browse, false),
        Some(aghist::action::Action::Quit)
    ));
}

#[test]
fn key_mapping_view_mode() {
    use crossterm::event::{KeyEvent, KeyModifiers};
    use aghist::event::map_key_event;

    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::ViewSession, false),
        Some(aghist::action::Action::BackToList)
    ));

    let key = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::ViewSession, false),
        Some(aghist::action::Action::ToggleToolCalls)
    ));

    let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::ViewSession, false),
        Some(aghist::action::Action::CopyResumeCommand)
    ));
}

#[test]
fn key_mapping_search_mode() {
    use crossterm::event::{KeyEvent, KeyModifiers};
    use aghist::event::map_key_event;

    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Search, false),
        Some(aghist::action::Action::SearchCancel)
    ));

    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Search, false),
        Some(aghist::action::Action::SearchSubmit)
    ));

    let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    assert!(matches!(
        map_key_event(key, AppMode::Search, false),
        Some(aghist::action::Action::SearchInput('a'))
    ));
}

// ─── Error resilience via real event loop ───────────────────────────────────

#[test]
fn corrupt_fixtures_no_crash() {
    let providers: Vec<Box<dyn HistoryProvider>> = vec![
        Box::new(ClaudeCodeProvider::new(vec![edge_cases_dir().join("claude")])),
        Box::new(CopilotCliProvider::new(vec![edge_cases_dir().join("copilot")])),
        Box::new(GeminiCliProvider::new(vec![edge_cases_dir().join("gemini")])),
        Box::new(CodexCliProvider::new(vec![edge_cases_dir().join("codex")])),
        Box::new(OpenCodeProvider::new(vec![edge_cases_dir().join("opencode")])),
    ];
    let mut app = App::new(providers, Config::default());
    let mut terminal = make_terminal();

    // Navigate and select through the event loop — should not panic
    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select a session
        KeyCode::Char('j'), // scroll
        KeyCode::Esc,       // back
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert!(app.session_count() > 0);
}

// ─── Live-updating search debounce ──────────────────────────────────────────

#[test]
fn search_input_debounces_until_idle() {
    use aghist::action::Action;

    let mut app = App::new(all_providers(), Config::default());
    app.load_sessions();

    app.dispatch(Action::SearchStart);
    app.dispatch(Action::SearchInput('t'));
    app.dispatch(Action::SearchInput('e'));
    app.dispatch(Action::SearchInput('s'));

    // Search is queued but not yet flushed.
    assert!(
        app.has_pending_search(),
        "fast typing should leave a pending search"
    );

    // Tick before the debounce window elapses → still pending.
    app.tick();
    assert!(
        app.has_pending_search(),
        "tick within debounce window should not flush"
    );

    // Wait past the debounce window, then tick → flushes.
    std::thread::sleep(std::time::Duration::from_millis(150));
    app.tick();
    assert!(
        !app.has_pending_search(),
        "tick after debounce window should flush"
    );
}

#[test]
fn search_submit_flushes_pending_search() {
    use aghist::action::Action;

    let mut app = App::new(all_providers(), Config::default());
    app.load_sessions();

    app.dispatch(Action::SearchStart);
    app.dispatch(Action::SearchInput('a'));
    assert!(app.has_pending_search());

    // Submit must not strand the queued query — flush before navigating.
    app.dispatch(Action::SearchSubmit);
    assert!(
        !app.has_pending_search(),
        "SearchSubmit should flush any pending debounced search"
    );
}

#[test]
fn search_cancel_drops_pending_search() {
    use aghist::action::Action;

    let mut app = App::new(all_providers(), Config::default());
    app.load_sessions();

    app.dispatch(Action::SearchStart);
    app.dispatch(Action::SearchInput('z'));
    assert!(app.has_pending_search());

    app.dispatch(Action::SearchCancel);
    assert!(
        !app.has_pending_search(),
        "SearchCancel should drop the pending debounced search"
    );
}

#[test]
fn nonexistent_dirs_empty_state() {
    let fake = PathBuf::from("/nonexistent/path");
    let providers: Vec<Box<dyn HistoryProvider>> = vec![
        Box::new(ClaudeCodeProvider::new(vec![fake.clone()])),
        Box::new(CopilotCliProvider::new(vec![fake.clone()])),
        Box::new(GeminiCliProvider::new(vec![fake.clone()])),
        Box::new(CodexCliProvider::new(vec![fake.clone()])),
        Box::new(OpenCodeProvider::new(vec![fake])),
    ];
    let mut app = App::new(providers, Config::default());
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.session_count(), 0);
}
