mod common;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use aghist::app::{App, AppMode};
use aghist::config::Config;
use aghist::provider::HistoryProvider;

use common::fixtures;
use common::helpers::{render_to_text, ScriptedEventSource};

fn make_app(providers: Vec<Box<dyn HistoryProvider>>) -> App {
    App::new(providers, Config::default())
}

fn make_terminal() -> Terminal<TestBackend> {
    let backend = TestBackend::new(120, 40);
    Terminal::new(backend).unwrap()
}

fn claude_providers(fixture: &fixtures::FixtureDir) -> Vec<Box<dyn HistoryProvider>> {
    vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )]
}

// ─── Basic workflows ────────────────────────────────────────────────────────

#[test]
fn quit_immediately() {
    let fixture = fixtures::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert!(app.should_quit());
    assert_eq!(app.mode(), AppMode::Browse);
    assert_eq!(app.session_count(), 1);
}

#[test]
fn browse_select_enters_view_mode() {
    let fixture = fixtures::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    // Select session, then quit from ViewSession mode
    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::ViewSession);

    let text = render_to_text(&terminal);
    assert!(
        text.contains("User") || text.contains("Assistant"),
        "session view should show message roles, got:\n{text}"
    );
}

#[test]
fn browse_select_and_back_returns_to_browse() {
    let fixture = fixtures::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select → ViewSession
        KeyCode::Esc,       // back → Browse
        KeyCode::Char('q'), // quit
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::Browse);
    assert_eq!(app.selected_index(), Some(0));
}

#[test]
fn navigate_moves_selection() {
    let (dirs, providers) = fixtures::all_generated_providers(3, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('j'), // move down → index 1
        KeyCode::Char('j'), // move down → index 2
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    drop(dirs);

    assert_eq!(app.selected_index(), Some(2));
}

#[test]
fn navigate_up_moves_selection_back() {
    let (dirs, providers) = fixtures::all_generated_providers(3, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('j'), // → 1
        KeyCode::Char('j'), // → 2
        KeyCode::Char('k'), // → 1
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    drop(dirs);

    assert_eq!(app.selected_index(), Some(1));
}

#[test]
fn scroll_in_session_view() {
    let fixture = fixtures::claude_single_session(10);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select session
        KeyCode::Char('j'), // scroll down
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('k'), // scroll up
        KeyCode::Esc,       // back
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::Browse);
}

#[test]
fn toggle_tool_calls_changes_rendered_output() {
    let fixture = fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-tools")
        .project("tools-project")
        .user("Read the file")
        .assistant_with_tool("Let me read it", "Read", r#"{"path":"main.rs"}"#)
        .tool_result("tool-002", "fn main() {}")
        .assistant("Here is the file content.")
        .done()
        .build();
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    // Enter session view, capture output without tool calls
    let events = ScriptedEventSource::from_keys(vec![KeyCode::Enter]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    let before = render_to_text(&terminal);

    // Toggle tool calls on, render again
    app.dispatch(aghist::action::Action::ToggleToolCalls);
    terminal.draw(|f| app.render(f)).unwrap();
    let after = render_to_text(&terminal);

    // The rendered output should change when tool calls are toggled
    assert_ne!(before, after, "toggling tool calls should change the rendered output");
}

#[test]
fn help_overlay_shows_keybindings() {
    let fixture = fixtures::claude_single_session(2);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('?')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::Help);

    let text = render_to_text(&terminal);
    assert!(
        text.contains("Help") || text.contains("Keybindings") || text.contains("Quit"),
        "help overlay should show key information, got:\n{text}"
    );
}

#[test]
fn help_toggle_returns_to_browse() {
    let fixture = fixtures::claude_single_session(2);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('?'), // open help
        KeyCode::Char('?'), // close help
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::Browse);
}

#[test]
fn filter_mode_shows_provider_list() {
    let (dirs, providers) = fixtures::all_generated_providers(1, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('f')]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    drop(dirs);

    assert_eq!(app.mode(), AppMode::Filter);

    let text = render_to_text(&terminal);
    assert!(
        text.contains("Claude") || text.contains("Copilot") || text.contains("Filter"),
        "filter overlay should show provider names, got:\n{text}"
    );
}

#[test]
fn filter_toggle_changes_rendered_output() {
    let (dirs, providers) = fixtures::all_generated_providers(1, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    // First, run to get a rendered view with all providers
    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    let before = render_to_text(&terminal);
    let total = app.session_count();

    // Now open filter, toggle first provider off, close filter
    let mut app2 = make_app({
        let (_, p) = fixtures::all_generated_providers(1, 2);
        p
    });
    let mut terminal2 = make_terminal();
    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('f'), // open filter
        KeyCode::Char(' '), // toggle first provider off
        KeyCode::Char('f'), // close filter
        KeyCode::Char('q'),
    ]);
    app2.run_with_event_source(&mut terminal2, events).unwrap();
    let after = render_to_text(&terminal2);
    drop(dirs);

    assert!(total > 0, "should have sessions");
    assert_eq!(app2.mode(), AppMode::Browse);
    // After filtering out a provider, rendered output should differ
    assert_ne!(before, after, "filtering a provider should change the displayed sessions");
}

#[test]
fn empty_state_renders_correctly() {
    let mut app = make_app(vec![]);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.session_count(), 0);
    assert_eq!(app.selected_index(), None);
}

#[test]
fn rapid_keys_preserves_valid_state() {
    let fixture = fixtures::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let mut keys = Vec::new();
    for _ in 0..20 {
        keys.push(KeyCode::Char('j'));
    }
    for _ in 0..20 {
        keys.push(KeyCode::Char('k'));
    }
    keys.push(KeyCode::Enter);
    for _ in 0..10 {
        keys.push(KeyCode::Char('j'));
    }
    keys.push(KeyCode::Esc);
    keys.push(KeyCode::Char('q'));

    let events = ScriptedEventSource::from_keys(keys);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::Browse);
    assert_eq!(app.session_count(), 1);
    // With only 1 session, selection should be clamped to index 0
    assert_eq!(app.selected_index(), Some(0));
}

#[test]
fn go_to_bottom_selects_last_item() {
    let (dirs, providers) = fixtures::all_generated_providers(3, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('G'), // go to bottom
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let total = app.session_count();
    drop(dirs);

    assert!(total > 1, "should have multiple sessions");
    assert_eq!(app.selected_index(), Some(total - 1));
}

#[test]
fn go_to_top_selects_first_item() {
    let (dirs, providers) = fixtures::all_generated_providers(3, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('G'), // go to bottom
        KeyCode::Char('g'), // go to top
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    drop(dirs);

    assert_eq!(app.selected_index(), Some(0));
}

#[test]
fn ctrl_c_quits_from_any_mode() {
    let fixture = fixtures::claude_single_session(2);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    // Enter session view, then Ctrl+C
    let events = ScriptedEventSource::new(vec![
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        )),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert!(app.should_quit());
    // Was in ViewSession when Ctrl+C hit
    assert_eq!(app.mode(), AppMode::ViewSession);
}

#[test]
fn session_view_shows_message_content() {
    let fixture = fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-content")
        .project("content-project")
        .user("What is the meaning of life?")
        .assistant("The answer is 42.")
        .done()
        .build();
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Enter]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("meaning of life") || text.contains("42"),
        "session view should show message content, got:\n{text}"
    );
}
