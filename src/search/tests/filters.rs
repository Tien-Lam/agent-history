use super::*;

#[test]
fn session_ids_with_messages_filters_by_role() {
    let dir = tempdir().unwrap();
    let index = SearchIndex::open_or_create(dir.path()).unwrap();

    let stub = StubProvider::new(Provider::ClaudeCode);
    let mut s_user = make_session("sess-user-only", "alpha");
    s_user.source_path = dir.path().join("sess-user-only.jsonl");
    std::fs::write(&s_user.source_path, "user").unwrap();
    let mut s_mixed = make_session("sess-mixed", "beta");
    s_mixed.source_path = dir.path().join("sess-mixed.jsonl");
    std::fs::write(&s_mixed.source_path, "mixed").unwrap();
    stub.add(s_user.clone(), vec![make_message("u-1", "user only msg")]);
    let mut asst = make_message("a-1", "assistant reply");
    asst.role = Role::Assistant;
    stub.add(s_mixed.clone(), vec![make_message("u-2", "user msg"), asst]);

    let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
    let s_mixed_key = s_mixed.identity_key();
    let sessions = vec![s_user, s_mixed];
    let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
    index.build_index(&sessions, &providers, &tx).unwrap();

    let only_assistant = index
        .session_ids_with_messages(Some(Role::Assistant), false)
        .unwrap();
    assert_eq!(only_assistant.len(), 1);
    assert!(only_assistant.contains(&s_mixed_key));

    let any_user = index
        .session_ids_with_messages(Some(Role::User), false)
        .unwrap();
    assert_eq!(any_user.len(), 2);

    // No filter → empty set (caller treats as "no constraint").
    let none = index.session_ids_with_messages(None, false).unwrap();
    assert!(none.is_empty());
}

#[test]
fn session_ids_with_messages_filters_by_has_tool_call() {
    let dir = tempdir().unwrap();
    let index = SearchIndex::open_or_create(dir.path()).unwrap();

    let stub = StubProvider::new(Provider::ClaudeCode);
    let mut s_plain = make_session("sess-plain", "alpha");
    s_plain.source_path = dir.path().join("sess-plain.jsonl");
    std::fs::write(&s_plain.source_path, "plain").unwrap();
    let mut s_with_tool = make_session("sess-with-tool", "beta");
    s_with_tool.source_path = dir.path().join("sess-with-tool.jsonl");
    std::fs::write(&s_with_tool.source_path, "with-tool").unwrap();
    stub.add(s_plain.clone(), vec![make_message("p-1", "no tool here")]);
    let mut tool_msg = make_message("t-1", "calling tool");
    tool_msg.role = Role::Assistant;
    tool_msg
        .content
        .push(ContentBlock::ToolUse(crate::model::ToolCall {
            id: "tc-1".to_string(),
            name: "fs.read".to_string(),
            arguments: "{\"path\":\"/x\"}".to_string(),
        }));
    stub.add(s_with_tool.clone(), vec![tool_msg]);

    let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
    let s_with_tool_key = s_with_tool.identity_key();
    let sessions = vec![s_plain, s_with_tool];
    let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
    index.build_index(&sessions, &providers, &tx).unwrap();

    let with_tools = index.session_ids_with_messages(None, true).unwrap();
    assert_eq!(with_tools.len(), 1);
    assert!(with_tools.contains(&s_with_tool_key));

    // Combined: assistant role AND has-tool-call → still just the tool session.
    let combined = index
        .session_ids_with_messages(Some(Role::Assistant), true)
        .unwrap();
    assert_eq!(combined.len(), 1);
    assert!(combined.contains(&s_with_tool_key));
}
