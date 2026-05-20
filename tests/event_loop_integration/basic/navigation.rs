use super::*;

#[test]
fn navigate_moves_selection() {
    let (dirs, providers) = fixtures::generated::all_generated_providers(3, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    drop(dirs);

    assert_eq!(app.selected_index(), Some(2));
}

#[test]
fn navigate_up_moves_selection_back() {
    let (dirs, providers) = fixtures::generated::all_generated_providers(3, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('k'),
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    drop(dirs);

    assert_eq!(app.selected_index(), Some(1));
}

#[test]
fn rapid_keys_preserves_valid_state() {
    let fixture = fixtures::claude::claude_single_session(4);
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
    assert_eq!(app.selected_index(), Some(0));
}

#[test]
fn go_to_bottom_selects_last_item() {
    let (dirs, providers) = fixtures::generated::all_generated_providers(3, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('G'), KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let total = app.session_count();
    drop(dirs);

    assert!(total > 1, "should have multiple sessions");
    assert_eq!(app.selected_index(), Some(total - 1));
}

#[test]
fn go_to_top_selects_first_item() {
    let (dirs, providers) = fixtures::generated::all_generated_providers(3, 2);
    let mut app = make_app(providers);
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('G'),
        KeyCode::Char('g'),
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    drop(dirs);

    assert_eq!(app.selected_index(), Some(0));
}
