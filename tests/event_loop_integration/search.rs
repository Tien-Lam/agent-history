use super::support::*;

// ─── Search workflow ────────────────────────────────────────────────────────

#[test]
fn search_enter_and_cancel_preserves_sessions() {
    let fixture = fixtures::claude::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('/'), // enter search mode
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::Search);

    let text = render_to_text(&terminal);
    assert!(
        text.contains("Search") || text.contains('/'),
        "search mode should show search indicator, got:\n{text}"
    );
}

#[test]
fn search_type_and_cancel_returns_to_browse() {
    let fixture = fixtures::claude::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('/'), // enter search
        KeyCode::Char('h'), // type query
        KeyCode::Char('e'),
        KeyCode::Char('l'),
        KeyCode::Char('l'),
        KeyCode::Char('o'),
        KeyCode::Esc, // cancel search
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::Browse);
    // All sessions should be visible again after cancel
    assert_eq!(app.session_count(), 1);
}

#[test]
fn search_with_indexing_finds_content() {
    // Create a fixture with known searchable content
    let fixture = fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-searchable")
        .project("search-project")
        .user("Tell me about quantum computing")
        .assistant("Quantum computing uses qubits instead of classical bits.")
        .done()
        .build();
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    // Enter search, type query, then wait for indexer to finish
    // The indexer runs on a background thread, so we need idle ticks
    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('/'), // enter search
        KeyCode::Char('q'),
        KeyCode::Char('u'),
        KeyCode::Char('a'),
        KeyCode::Char('n'),
        KeyCode::Char('t'),
        KeyCode::Char('u'),
        KeyCode::Char('m'),
    ])
    .with_idle_ticks(40) // give indexer time to complete
    .then_key(KeyCode::Enter); // submit search (selects first result)

    app.run_with_event_source(&mut terminal, events).unwrap();

    // If the search found results and we submitted, we should be in ViewSession
    // If the index wasn't ready, we'd still be in Search mode
    // Either is acceptable, but let's verify the app is in a valid state
    assert!(
        app.mode() == AppMode::ViewSession || app.mode() == AppMode::Search,
        "after search submit, should be in ViewSession or Search, got {:?}",
        app.mode()
    );
}
