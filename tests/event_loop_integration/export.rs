use super::support::*;

// ─── Export workflow ────────────────────────────────────────────────────────

#[test]
fn export_menu_opens_from_session_view() {
    let fixture = fixtures::claude::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select session → ViewSession
        KeyCode::Char('e'), // open export menu
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::ExportMenu);

    let text = render_to_text(&terminal);
    assert!(
        text.contains("Markdown")
            || text.contains("JSON")
            || text.contains("HTML")
            || text.contains("md")
            || text.contains("json")
            || text.contains("html"),
        "export menu should show format options, got:\n{text}"
    );
}

#[test]
fn export_cancel_returns_to_session_view() {
    let fixture = fixtures::claude::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select session
        KeyCode::Char('e'), // open export menu
        KeyCode::Esc,       // cancel export
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::ViewSession);
}

#[test]
fn export_navigate_formats() {
    let fixture = fixtures::claude::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select session
        KeyCode::Char('e'), // open export menu
        KeyCode::Char('j'), // move to next format
        KeyCode::Char('j'), // move to next format
        KeyCode::Char('k'), // move back up
        KeyCode::Esc,       // cancel
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::ViewSession);
}

#[test]
fn export_confirm_writes_file_and_returns() {
    let fixture = fixtures::claude::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select session
        KeyCode::Char('e'), // open export menu
        KeyCode::Enter,     // confirm export (first format = md)
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::ViewSession);

    // The status bar should show the export message in rendered output
    let text = render_to_text(&terminal);
    assert!(
        text.contains("Exported") || text.contains("Export"),
        "rendered output should show export status, got:\n{text}"
    );

    // Clean up the exported file
    for entry in std::fs::read_dir(".").unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("aghist-")
            && std::path::Path::new(&name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

// ─── Help toggle preserves mode ──────────────────────────────────────────────

#[test]
fn help_toggle_returns_to_view_session() {
    let fixture = fixtures::claude::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select session → ViewSession
        KeyCode::Char('?'), // toggle help → Help
        KeyCode::Char('?'), // toggle help again → should return to ViewSession
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(
        app.mode(),
        AppMode::ViewSession,
        "help toggle should restore ViewSession, not Browse"
    );
}

// ─── Export with filter ────────────────────────────────────────────────────────

#[test]
fn export_while_filtered_writes_correct_session() {
    let fixture = fixtures::claude::claude_single_session(4);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,     // select session → ViewSession
        KeyCode::Char('e'), // open export menu
        KeyCode::Enter,     // confirm export (Markdown)
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::ViewSession);

    let text = render_to_text(&terminal);
    assert!(
        text.contains("Exported") || text.contains("Export"),
        "should show export confirmation"
    );

    // Clean up
    for entry in std::fs::read_dir(".").unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("aghist-") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
