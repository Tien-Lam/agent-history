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

#[test]
fn opencode_incremental_reindex_tracks_session_part_changes() {
    let fixture = fixtures::opencode::OpenCodeFixtureBuilder::new()
        .add_session("oc-alpha")
        .raw_message(
            "msg-alpha",
            r#"{"id":"msg-alpha","role":"assistant","timestamp":"2025-01-01T00:00:00Z","summary":{"title":"fallback alpha"}}"#,
        )
        .done()
        .add_session("oc-bravo")
        .raw_message(
            "msg-bravo",
            r#"{"id":"msg-bravo","role":"assistant","timestamp":"2025-01-01T00:01:00Z","summary":{"title":"fallback bravo"}}"#,
        )
        .done()
        .build();
    let alpha_part_dir = fixture.base_path.join("part").join("msg-alpha");
    fs::create_dir_all(&alpha_part_dir).unwrap();
    fs::write(
        alpha_part_dir.join("part-001.json"),
        r#"{"type":"text","text":"omegaoriginal"}"#,
    )
    .unwrap();
    let bravo_part_dir = fixture.base_path.join("part").join("msg-bravo");
    fs::create_dir_all(&bravo_part_dir).unwrap();
    fs::write(
        bravo_part_dir.join("part-001.json"),
        r#"{"type":"text","text":"bravo original"}"#,
    )
    .unwrap();

    let providers: Vec<Box<dyn HistoryProvider>> =
        vec![Box::new(OpenCodeProvider::new(vec![fixture
            .base_path
            .clone()]))];
    let mut sessions = Vec::new();
    for provider in &providers {
        sessions.extend(provider.discover_sessions().unwrap());
    }

    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();
    let (tx, _rx) = crossbeam_channel::unbounded();
    let stats1 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert_eq!(stats1.sessions_indexed, 2);
    assert!(!index.search("omegaoriginal", 10).unwrap().is_empty());
    assert!(!index.search("bravo original", 10).unwrap().is_empty());

    fs::write(
        alpha_part_dir.join("part-001.json"),
        r#"{"type":"text","text":"zetarevised"}"#,
    )
    .unwrap();

    let mut sessions = Vec::new();
    for provider in &providers {
        sessions.extend(provider.discover_sessions().unwrap());
    }
    let stats2 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert_eq!(
        stats2.updated, 1,
        "only the session whose part content changed should be reindexed"
    );
    assert_eq!(stats2.unchanged, 1);
    assert!(!index.search("zetarevised", 10).unwrap().is_empty());
    assert!(index.search("omegaoriginal", 10).unwrap().is_empty());
    assert!(!index.search("bravo original", 10).unwrap().is_empty());
}
