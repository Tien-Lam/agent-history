use super::*;

#[test]
fn corrupt_fixtures_no_crash() {
    let providers: Vec<Box<dyn HistoryProvider>> = vec![
        Box::new(ClaudeCodeProvider::new(vec![
            edge_cases_dir().join("claude")
        ])),
        Box::new(CopilotCliProvider::new(vec![
            edge_cases_dir().join("copilot")
        ])),
        Box::new(GeminiCliProvider::new(
            vec![edge_cases_dir().join("gemini")],
        )),
        Box::new(CodexCliProvider::new(vec![edge_cases_dir().join("codex")])),
        Box::new(OpenCodeProvider::new(vec![
            edge_cases_dir().join("opencode")
        ])),
    ];
    let mut app = App::new(providers, Config::default());
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![
        KeyCode::Enter,
        KeyCode::Char('j'),
        KeyCode::Esc,
        KeyCode::Char('q'),
    ]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert!(app.session_count() > 0);
}

#[test]
fn nonexistent_dirs_empty_state() {
    let fake = PathBuf::from("/nonexistent/path");
    let providers: Vec<Box<dyn HistoryProvider>> = vec![
        Box::new(ClaudeCodeProvider::new(vec![fake.clone()])),
        Box::new(CopilotCliProvider::new(vec![fake.clone()])),
        Box::new(GeminiCliProvider::new(vec![fake.clone()])),
        Box::new(CodexCliProvider::new(vec![fake.clone()])),
        Box::new(OpenCodeProvider::new(vec![fake])),
    ];
    let mut app = App::new(providers, Config::default());
    let mut terminal = make_terminal();

    let events = ScriptedEventSource::from_keys(vec![KeyCode::Char('q')]);
    app.run_with_event_source(&mut terminal, events).unwrap();

    assert_eq!(app.session_count(), 0);
}
