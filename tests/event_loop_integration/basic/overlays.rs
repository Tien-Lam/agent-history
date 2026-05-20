use super::*;

#[test]
fn help_overlay_shows_keybindings() {
    let fixture = fixtures::claude::claude_single_session(2);
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
    let fixture = fixtures::claude::claude_single_session(2);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('?'),
        KeyCode::Char('?'),
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::Browse);
}
