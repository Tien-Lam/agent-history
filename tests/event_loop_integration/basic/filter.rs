use super::*;

#[test]
fn filter_mode_shows_provider_list() {
    let (dirs, providers) = fixtures::generated::all_generated_providers(1, 2);
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
    let (dirs1, providers1) = fixtures::generated::all_generated_providers(1, 2);
    let mut app1 = make_app(providers1);
    let mut terminal1 = make_terminal();
    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app1.run_with_event_source(&mut terminal1, events).unwrap();
    let before = render_to_text(&terminal1);
    let total = app1.session_count();
    drop(dirs1);

    let (dirs2, providers2) = fixtures::generated::all_generated_providers(1, 2);
    let mut app2 = make_app(providers2);
    let mut terminal2 = make_terminal();
    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('f'),
        KeyCode::Char(' '),
        KeyCode::Char('f'),
        KeyCode::Char('q'),
    ]);
    app2.run_with_event_source(&mut terminal2, events).unwrap();
    let after = render_to_text(&terminal2);
    drop(dirs2);

    assert!(total > 0, "should have sessions");
    assert_eq!(app2.mode(), AppMode::Browse);
    assert_ne!(
        before, after,
        "filtering a provider should change the displayed sessions"
    );
}
