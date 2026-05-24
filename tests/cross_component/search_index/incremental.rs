use super::super::support::*;

#[test]
fn search_incremental_reindex_after_file_change() {
    let tmp = tempfile::tempdir().unwrap();
    let index_dir = tmp.path().join("index");
    let fixture_dir = tmp.path().join("claude");

    let src = fixtures_dir().join("claude");
    copy_dir_recursive(&src, &fixture_dir);

    let providers: Vec<Box<dyn HistoryProvider>> =
        vec![Box::new(ClaudeCodeProvider::new(vec![fixture_dir.clone()]))];

    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let index = SearchIndex::open_or_create(&index_dir).unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();

    let stats1 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert!(stats1.sessions_indexed > 0);
    assert!(stats1.messages_indexed > 0);

    let hits = index.search("build error", 10).unwrap();
    assert!(!hits.is_empty());

    let hits = index.search("quantum entanglement refactor", 10).unwrap();
    assert!(hits.is_empty());

    let session_file = fixture_dir
        .join("projects")
        .join("test-project")
        .join("session-abc123.jsonl");

    thread::sleep(Duration::from_millis(1100));

    let new_line = r#"{"type":"user","uuid":"msg-005","timestamp":"2025-04-08T10:01:00Z","message":{"role":"user","content":"Apply the quantum entanglement refactor to the parser"},"cwd":"/home/user/project"}"#;
    let mut content = fs::read_to_string(&session_file).unwrap();
    content.push_str(new_line);
    content.push('\n');
    fs::write(&session_file, content).unwrap();

    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let stats2 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert!(
        stats2.sessions_indexed > 0,
        "changed file should be re-indexed"
    );
    assert!(
        stats2.updated >= 1,
        "modified session must be classified as 'updated'"
    );
    assert_eq!(stats2.added, 0, "no new sessions, none should be 'added'");

    let hits = index.search("quantum entanglement refactor", 10).unwrap();
    assert!(
        !hits.is_empty(),
        "new content should appear after incremental reindex"
    );
    assert_eq!(hits[0].session_id(), "session-abc123");
    assert_eq!(hits[0].message_id(), "msg-005");

    let hits = index.search("build error", 10).unwrap();
    assert!(
        !hits.is_empty(),
        "original content should survive incremental reindex"
    );
}
