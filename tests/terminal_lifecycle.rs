mod common;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
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

// ─── Terminal size variations ───────────────────────────────────────────────

#[test]
fn render_at_minimum_size() {
    let fixture = fixtures::claude_single_session(4);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    assert!(app.should_quit());
}

#[test]
fn render_at_large_size() {
    let fixture = fixtures::claude_single_session(4);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let backend = TestBackend::new(300, 100);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    assert!(app.should_quit());
}

#[test]
fn render_at_single_row() {
    let fixture = fixtures::claude_single_session(2);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let backend = TestBackend::new(80, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    assert!(app.should_quit());
}

// ─── Resize events ─────────────────────────────────────────────────────────

#[test]
fn resize_event_does_not_crash() {
    let fixture = fixtures::claude_single_session(4);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::new(vec![
        Event::Resize(80, 24),
        Event::Resize(200, 60),
        Event::Resize(40, 10),
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    assert!(app.should_quit());
}

#[test]
fn rapid_resize_events_no_crash() {
    let fixture = fixtures::claude_single_session(4);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut events: Vec<Event> = (0..20)
        .map(|i| Event::Resize(40 + i * 10, 10 + i * 3))
        .collect();
    events.push(Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('q'),
        KeyModifiers::NONE,
        KeyEventKind::Press,
    )));
    let events = ScriptedEventSource::new(events);
    app.run_with_event_source(&mut terminal, events).unwrap();
    assert!(app.should_quit());
}

// ─── Mode transitions across resize ────────────────────────────────────────

#[test]
fn resize_during_session_view() {
    let fixture = fixtures::claude_single_session(6);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::new(vec![
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
        Event::Resize(60, 20),
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
        Event::Resize(200, 50),
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Esc,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    assert!(app.should_quit());
}

#[test]
fn resize_during_help_overlay() {
    let fixture = fixtures::claude_single_session(2);
    let providers: Vec<Box<dyn HistoryProvider>> = vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )];
    let mut app = make_app(providers);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::new(vec![
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('?'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
        Event::Resize(60, 20),
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('?'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    assert!(app.should_quit());
}

// ─── Empty state resilience ────────────────────────────────────────────────

#[test]
fn empty_state_at_tiny_terminal() {
    let mut app = make_app(vec![]);
    let backend = TestBackend::new(10, 3);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    assert!(app.should_quit());
}

#[test]
fn navigation_in_empty_state_no_crash() {
    let mut app = make_app(vec![]);
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('j'), // navigate down on empty list
        KeyCode::Char('k'), // navigate up on empty list
        KeyCode::Enter,     // select on empty list
        KeyCode::Char('G'), // go to bottom on empty
        KeyCode::Char('g'), // go to top on empty
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    assert!(app.should_quit());
}
