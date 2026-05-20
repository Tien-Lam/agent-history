use super::*;

#[test]
fn quit_immediately() {
    let fixture = fixtures::claude::claude_single_session(4);
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
    let fixture = fixtures::claude::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Enter, KeyCode::Char('q')]);
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
    let fixture = fixtures::claude::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events =
        ScriptedEventSource::from_keys(vec![KeyCode::Enter, KeyCode::Esc, KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::Browse);
    assert_eq!(app.selected_index(), Some(0));
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
fn ctrl_c_quits_from_any_mode() {
    let fixture = fixtures::claude::claude_single_session(2);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

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
    assert_eq!(app.mode(), AppMode::ViewSession);
}
