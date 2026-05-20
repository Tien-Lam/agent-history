use super::*;

#[test]
fn scroll_in_session_view() {
    let fixture = fixtures::claude::claude_single_session(10);
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('k'),
        KeyCode::Esc,
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.mode(), AppMode::Browse);
}

#[test]
fn toggle_tool_calls_changes_rendered_output() {
    let fixture = fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-tools")
        .project("tools-project")
        .user("Read the file")
        .assistant_with_tool("Let me read it", "Read", r#"{"path":"main.rs"}"#)
        .tool_result("tool-002", "fn main() {}")
        .assistant("Here is the file content.")
        .done()
        .build();

    let mut app1 = make_app(claude_providers(&fixture));
    let mut terminal1 = make_terminal();
    let events = ScriptedEventSource::from_keys(vec![KeyCode::Enter]);
    app1.run_with_event_source(&mut terminal1, events).unwrap();
    let without_tools = render_to_text(&terminal1);

    let mut app2 = make_app(claude_providers(&fixture));
    let mut terminal2 = make_terminal();
    let events = ScriptedEventSource::from_keys(vec![KeyCode::Enter, KeyCode::Char('t')]);
    app2.run_with_event_source(&mut terminal2, events).unwrap();
    let with_tools = render_to_text(&terminal2);

    assert_ne!(
        without_tools, with_tools,
        "toggling tool calls should change the rendered output"
    );
}

#[test]
fn session_view_shows_message_content() {
    let fixture = fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-content")
        .project("content-project")
        .user("What is the meaning of life?")
        .assistant("The answer is 42.")
        .done()
        .build();
    let mut app = make_app(claude_providers(&fixture));
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Enter]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    let text = render_to_text(&terminal);
    assert!(
        text.contains("meaning of life") || text.contains("42"),
        "session view should show message content, got:\n{text}"
    );
}
