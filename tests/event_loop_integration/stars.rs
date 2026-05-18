use super::support::*;

// ─── Stars / bookmarks ─────────────────────────────────────────────────────────

use aghist::stars::StarStore;

fn make_app_with_stars(
    providers: Vec<Box<dyn HistoryProvider>>,
    stars_path: &std::path::Path,
) -> App {
    App::with_stars(
        providers,
        Config::default(),
        StarStore::load_from(stars_path),
    )
}

#[test]
fn toggle_star_persists_across_runs() {
    let dir = tempfile::tempdir().unwrap();
    let stars_path = dir.path().join("metadata.db");

    // Run 1: load sessions, focus first one, press 's' to star, then quit.
    let (dirs1, providers1) = fixtures::generated::all_generated_providers(1, 2);
    let mut app1 = make_app_with_stars(providers1, &stars_path);
    let mut terminal1 = make_terminal();
    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('s'), KeyCode::Char('q')]);
    app1.run_with_event_source(&mut terminal1, events).unwrap();
    drop(dirs1);

    // The star should now be on disk.
    assert!(stars_path.exists(), "metadata.db should be written");

    // Run 2: confirm the persisted star reloads.
    let store = StarStore::load_from(&stars_path);
    assert_eq!(store.count(), 1);
}

#[test]
fn toggle_star_twice_unstars() {
    let dir = tempfile::tempdir().unwrap();
    let stars_path = dir.path().join("metadata.db");

    let (dirs, providers) = fixtures::generated::all_generated_providers(1, 2);
    let mut app = make_app_with_stars(providers, &stars_path);
    let mut terminal = make_terminal();
    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('s'), // star
        KeyCode::Char('s'), // unstar
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    drop(dirs);

    let store = StarStore::load_from(&stars_path);
    assert_eq!(store.count(), 0, "second 's' should remove the star");
}

#[test]
fn starred_only_filter_hides_unstarred_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let stars_path = dir.path().join("metadata.db");

    let (dirs, providers) = fixtures::generated::all_generated_providers(1, 2);
    let mut app = make_app_with_stars(providers, &stars_path);
    let mut terminal = make_terminal();

    // Star only the focused (first) session, then enable the
    // starred-only filter and verify the rendered list shrinks
    // to a single entry.
    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('s'), // star session 0
        KeyCode::Char('f'), // open filter
        // Cursor lands on the first provider; jump to the starred-only
        // toggle which sits after all providers + 3 text fields + 2
        // message-level toggles (role, has-tool-call).
        KeyCode::Char('G'), // GoToBottom is unmapped in Filter mode → no-op
        // Walk down: 10 providers + 3 text fields + 2 message toggles = 15
        // -> press j 15 times.
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char(' '), // toggle "starred only"
        KeyCode::Char('f'), // close filter (applies it)
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let total = app.session_count();
    drop(dirs);

    let text = render_to_text(&terminal);
    // Sessions count text comes from the SessionList block title
    // " Sessions (N) ". With starred-only enabled and exactly one
    // starred session, the rendered title must read "Sessions (1)"
    // even though more sessions exist in total.
    assert!(total > 1, "fixture should have multiple sessions");
    assert!(
        text.contains("Sessions (1)"),
        "starred-only filter should show exactly 1 session, got:\n{text}"
    );
    // The starred indicator (★) should be visible in the list.
    assert!(
        text.contains('\u{2605}'),
        "starred sessions should render a ★ marker, got:\n{text}"
    );
}

#[test]
fn star_marker_appears_in_session_list() {
    let dir = tempfile::tempdir().unwrap();
    let stars_path = dir.path().join("metadata.db");

    let fixture = fixtures::claude::claude_single_session(2);
    let mut app = make_app_with_stars(claude_providers(&fixture), &stars_path);
    let mut terminal = make_terminal();

    // Before: no star marker.
    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();
    let before = render_to_text(&terminal);
    assert!(!before.contains('\u{2605}'), "no star initially");

    // Toggle star, then render again.
    let mut app2 = make_app_with_stars(claude_providers(&fixture), &stars_path);
    let mut terminal2 = make_terminal();
    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('s'), KeyCode::Char('q')]);
    app2.run_with_event_source(&mut terminal2, events).unwrap();
    let after = render_to_text(&terminal2);
    assert!(
        after.contains('\u{2605}'),
        "star marker should appear after toggle, got:\n{after}"
    );
}
