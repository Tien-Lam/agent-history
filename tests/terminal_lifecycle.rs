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

fn claude_providers(fixture: &fixtures::FixtureDir) -> Vec<Box<dyn HistoryProvider>> {
    vec![Box::new(
        aghist::provider::claude_code::ClaudeCodeProvider::new(vec![fixture.base_path.clone()]),
    )]
}

// ─── Terminal size variations ───────────────────────────────────────────────

#[test]
fn render_at_minimum_size_shows_content() {
    let fixture = fixtures::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let backend = TestBackend::new(40, 8);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.session_count(), 1);
    let text = render_to_text(&terminal);
    // Even at small size, something should render (not blank)
    let non_empty_lines = text.lines().filter(|l| !l.trim().is_empty()).count();
    assert!(
        non_empty_lines > 0,
        "should render content even at small size, got:\n{text}"
    );
}

#[test]
fn render_at_large_size_shows_content() {
    let fixture = fixtures::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let backend = TestBackend::new(300, 100);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    // At large size, should show session info
    assert!(
        text.contains("Claude") || text.contains("gen-project") || text.contains("session"),
        "large terminal should display session info, got:\n{text}"
    );
}

#[test]
fn render_at_single_row_no_panic() {
    let fixture = fixtures::claude_single_session(2);
    let mut app = make_app(claude_providers(&fixture));
    let backend = TestBackend::new(80, 1);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    // Main assertion: this doesn't panic with a 1-row terminal
    app.run_with_event_source(&mut terminal, events).unwrap();
    assert!(app.should_quit());
}

// ─── Resize events ─────────────────────────────────────────────────────────

#[test]
fn resize_event_keeps_app_functional() {
    let fixture = fixtures::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
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

    // After resize events, app should still be in a valid state
    assert_eq!(app.mode(), AppMode::Browse);
    assert_eq!(app.session_count(), 1);
    assert_eq!(app.selected_index(), Some(0));
}

#[test]
fn rapid_resize_preserves_state() {
    let fixture = fixtures::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
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

    assert_eq!(app.mode(), AppMode::Browse);
    assert_eq!(app.session_count(), 1);
    assert_eq!(app.selected_index(), Some(0));
}

// ─── Mode transitions across resize ────────────────────────────────────────

#[test]
fn resize_during_session_view_preserves_mode() {
    let fixture = fixtures::claude_single_session(6);
    let mut app = make_app(claude_providers(&fixture));
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::new(vec![
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
        Event::Resize(60, 20),
        // After resize, should still be in ViewSession — verify by scrolling
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
        Event::Resize(200, 50),
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    // Should still be in ViewSession — resize shouldn't change mode
    assert_eq!(app.mode(), AppMode::ViewSession);

    let text = render_to_text(&terminal);
    assert!(
        text.contains("User") || text.contains("Assistant"),
        "should still show session content after resize, got:\n{text}"
    );
}

#[test]
fn resize_during_help_overlay_keeps_help_visible() {
    let fixture = fixtures::claude_single_session(2);
    let mut app = make_app(claude_providers(&fixture));
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
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    // Help mode should persist through resize — 'q' is ignored in help mode
    assert_eq!(app.mode(), AppMode::Help);
}

// ─── Empty state resilience ────────────────────────────────────────────────

#[test]
fn empty_state_at_tiny_terminal_renders() {
    let mut app = make_app(vec![]);
    let backend = TestBackend::new(10, 3);
    let mut terminal = Terminal::new(backend).unwrap();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.session_count(), 0);
    // Renders without panic even at tiny terminal size with no data
}

#[test]
fn navigation_in_empty_state_preserves_none_selection() {
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

    // After all navigation attempts on empty list, selection should still be None
    assert_eq!(app.selected_index(), None);
    assert_eq!(app.mode(), AppMode::Browse);
    assert_eq!(app.session_count(), 0);
}

// ─── Different terminal dimensions produce different layouts ────────────────

#[test]
fn wide_vs_narrow_terminal_differ() {
    let fixture = fixtures::claude_single_session(4);

    // Render at wide size
    let mut app1 = make_app(claude_providers(&fixture));
    let backend1 = TestBackend::new(200, 30);
    let mut terminal1 = Terminal::new(backend1).unwrap();
    let events1 = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app1.run_with_event_source(&mut terminal1, events1).unwrap();
    let wide_text = render_to_text(&terminal1);

    // Render at narrow size
    let mut app2 = make_app(claude_providers(&fixture));
    let backend2 = TestBackend::new(40, 30);
    let mut terminal2 = Terminal::new(backend2).unwrap();
    let events2 = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app2.run_with_event_source(&mut terminal2, events2).unwrap();
    let narrow_text = render_to_text(&terminal2);

    // Wide and narrow should produce different layouts
    assert_ne!(
        wide_text, narrow_text,
        "200-col and 40-col terminals should produce different layouts"
    );
}
