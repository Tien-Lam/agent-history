use super::super::support::*;

#[test]
fn search_roundtrip_verifies_message_ids() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    index.build_index(&sessions, &providers, &tx).unwrap();

    let hits = index.search("build error", 50).unwrap();
    assert!(!hits.is_empty());

    let claude_hits: Vec<_> = hits
        .iter()
        .filter(|h| h.session_id == "session-abc123")
        .collect();
    assert!(
        claude_hits.len() >= 2,
        "expected at least 2 hits from session-abc123, got {}",
        claude_hits.len()
    );

    let hit_message_ids: Vec<&str> = claude_hits.iter().map(|h| h.message_id.as_str()).collect();
    assert!(
        hit_message_ids.contains(&"msg-001"),
        "should find user message msg-001, got: {hit_message_ids:?}"
    );
    assert!(
        hit_message_ids.contains(&"msg-002"),
        "should find assistant message msg-002, got: {hit_message_ids:?}"
    );

    for hit in &hits {
        assert!(hit.score > 0.0, "score should be positive");
    }

    let hits = index.search("missing semicolon", 10).unwrap();
    assert!(
        !hits.is_empty(),
        "should find 'missing semicolon' in thinking block"
    );
    let claude_hit = hits
        .iter()
        .find(|h| h.session_id == "session-abc123")
        .expect("should have a hit from session-abc123");
    assert_eq!(claude_hit.message_id, "msg-004");
}

#[test]
fn search_index_finds_tool_output() {
    let token = "zorpglyph42";
    let fixture = fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-tool-out")
        .project("tooloutput-project")
        .user("Run the test")
        .assistant_with_tool("running", "Bash", r#"{"command":"cargo test"}"#)
        .tool_result("tool-002", &format!("error[E0001]: {token} expected here"))
        .done()
        .build();

    let providers: Vec<Box<dyn HistoryProvider>> =
        vec![Box::new(ClaudeCodeProvider::new(vec![fixture
            .base_path
            .clone()]))];
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();
    index.build_index(&sessions, &providers, &tx).unwrap();

    let hits = index.search(token, 10).unwrap();
    assert!(
        !hits.is_empty(),
        "search for tool-output-only token '{token}' should return a hit"
    );
    assert_eq!(hits[0].session_id, "session-tool-out");
    assert!(
        hits[0].snippet.contains(token),
        "snippet should reflect the matching tool output, got: {:?}",
        hits[0].snippet
    );
}
