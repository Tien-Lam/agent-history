use super::*;

#[test]
fn resume_command_claude_code() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('y')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("claude --resume session-abc123"),
        "status bar should show Claude resume command, got:\n{text}"
    );
}

#[test]
fn resume_command_copilot_cli() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('j'), KeyCode::Char('y')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("copilot --resume=copilot-session-001"),
        "status bar should show Copilot resume command, got:\n{text}"
    );
}

#[test]
fn resume_command_gemini_cli() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('y'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("gemini --resume gemini-sess-001"),
        "status bar should show Gemini resume command, got:\n{text}"
    );
}

#[test]
fn resume_command_codex_cli() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Char('G'),
        KeyCode::Char('k'),
        KeyCode::Char('y'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("codex resume test123"),
        "status bar should show Codex resume command, got:\n{text}"
    );
}

#[test]
fn resume_command_opencode() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('G'), KeyCode::Char('y')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("opencode --session sess-001"),
        "status bar should show OpenCode resume command, got:\n{text}"
    );
}

#[test]
fn resume_command_in_view_mode() {
    let mut app = App::new(all_providers(), Config::default());
    let mut terminal = wide_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Enter, KeyCode::Char('y')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::ViewSession);
    let text = render_to_text(&terminal);
    assert!(
        text.contains("claude --resume session-abc123"),
        "resume command should work from view mode, got:\n{text}"
    );
}

#[test]
fn resume_command_no_selection() {
    let providers: Vec<Box<dyn HistoryProvider>> = Vec::new();
    let mut app = App::new(providers, Config::default());
    let mut terminal = wide_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('y')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        !text.contains("--resume"),
        "should not show resume command with no sessions, got:\n{text}"
    );
}
