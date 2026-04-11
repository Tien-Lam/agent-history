mod common;

use crossterm::event::KeyCode;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use aghist::app::App;
use aghist::config::Config;
use aghist::provider::HistoryProvider;

use common::fixtures;
use common::helpers::ScriptedEventSource;

fn make_app(providers: Vec<Box<dyn HistoryProvider>>) -> App {
    App::new(providers, Config::default())
}

fn make_terminal() -> Terminal<TestBackend> {
    let backend = TestBackend::new(120, 40);
    Terminal::new(backend).unwrap()
}

// ─── Basic workflows ────────────────────────────────────────────────────────

#[test]
fn quit_immediately() {
    let fixture = fixtures::claude_single_session(4);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert!(app.should_quit());
}

#[test]
fn browse_select_and_back() {
    let fixture = fixtures::claude_single_session(4);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select first session
        KeyCode::Esc,       // back to list
        KeyCode::Char('q'), // quit
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert!(app.should_quit());
}

#[test]
fn browse_navigate_and_select() {
    let (dirs, providers) = fixtures::all_generated_providers(2, 4);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('j'), // move down
        KeyCode::Char('j'), // move down
        KeyCode::Char('k'), // move up
        KeyCode::Enter,     // select
        KeyCode::Esc,       // back
        KeyCode::Char('q'), // quit
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    drop(dirs);

    assert!(app.should_quit());
}

#[test]
fn scroll_in_session_view() {
    let fixture = fixtures::claude_single_session(10);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select session
        KeyCode::Char('j'), // scroll down
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('k'), // scroll up
        KeyCode::Esc,       // back
        KeyCode::Char('q'), // quit
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert!(app.should_quit());
}

#[test]
fn toggle_tool_calls_in_session() {
    let fixture = fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-tools")
        .project("tools-project")
        .user("Read the file")
        .assistant_with_tool("Let me read it", "Read", r#"{"path":"main.rs"}"#)
        .tool_result("tool-002", "fn main() {}")
        .assistant("Here is the file content.")
        .done()
        .build();
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select session
        KeyCode::Char('t'), // toggle tool calls
        KeyCode::Char('t'), // toggle back
        KeyCode::Esc,       // back
        KeyCode::Char('q'), // quit
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert!(app.should_quit());
}

#[test]
fn help_toggle() {
    let fixture = fixtures::claude_single_session(2);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('?'), // open help
        KeyCode::Char('?'), // close help
        KeyCode::Char('q'), // quit
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert!(app.should_quit());
}

#[test]
fn filter_toggle_provider() {
    let (dirs, providers) = fixtures::all_generated_providers(1, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('f'), // open filter
        KeyCode::Char(' '), // toggle first provider
        KeyCode::Char('f'), // close filter
        KeyCode::Char('q'), // quit
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    drop(dirs);

    assert!(app.should_quit());
}

#[test]
fn empty_state_quits_cleanly() {
    let mut app = make_app(vec![]);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert!(app.should_quit());
}

#[test]
fn rapid_keys_no_crash() {
    let fixture = fixtures::claude_single_session(4);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
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

    assert!(app.should_quit());
}

#[test]
fn go_to_top_and_bottom_in_browse() {
    let (dirs, providers) = fixtures::all_generated_providers(3, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('G'), // go to bottom
        KeyCode::Char('g'), // go to top
        KeyCode::Char('q'), // quit
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    drop(dirs);

    assert!(app.should_quit());
}

#[test]
fn ctrl_c_quits() {
    use crossterm::event::{Event, KeyEvent, KeyEventKind, KeyModifiers};

    let fixture = fixtures::claude_single_session(2);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::new(vec![Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
        KeyEventKind::Press,
    ))]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert!(app.should_quit());
}
