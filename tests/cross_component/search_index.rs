use super::support::*;

#[test]
fn search_index_build_and_query() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    let stats = index.build_index(&sessions, &providers, &tx).unwrap();
    assert!(stats.messages_indexed > 0);

    // Search for content we know exists in Claude fixture
    let hits = index.search("build error", 10).unwrap();
    assert!(
        !hits.is_empty(),
        "should find 'build error' in Claude fixture"
    );
    assert_eq!(hits[0].session_id, "session-abc123");
    assert!(!hits[0].snippet.is_empty());

    // Search for content in Gemini fixture
    let hits = index.search("async", 10).unwrap();
    assert!(!hits.is_empty(), "should find 'async' in Gemini fixture");

    // Search for content that doesn't exist
    let hits = index.search("xyznonexistent", 10).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn search_index_incremental_rebuild() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();

    // First build indexes everything
    let stats1 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert!(stats1.sessions_indexed > 0);
    assert_eq!(
        stats1.added, stats1.sessions_indexed,
        "first run is all 'added'"
    );
    assert_eq!(stats1.updated, 0);
    assert_eq!(stats1.unchanged, 0);

    // Second build should skip (mtime unchanged)
    let stats2 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert_eq!(
        stats2.sessions_indexed, 0,
        "no sessions should need re-indexing"
    );
    assert_eq!(stats2.added, 0);
    assert_eq!(stats2.updated, 0);
    assert_eq!(
        stats2.unchanged,
        sessions.len(),
        "all sessions reported as unchanged"
    );

    // Search still works after incremental rebuild
    let hits = index.search("build error", 10).unwrap();
    assert!(!hits.is_empty());
}

#[test]
fn search_index_clear_and_rebuild() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    index.build_index(&sessions, &providers, &tx).unwrap();

    // Clear wipes everything
    index.clear().unwrap();
    let hits = index.search("build error", 10).unwrap();
    assert!(hits.is_empty(), "should find nothing after clear");

    // Rebuild restores results
    index.build_index(&sessions, &providers, &tx).unwrap();
    let hits = index.search("build error", 10).unwrap();
    assert!(!hits.is_empty(), "should find results after rebuild");
}

#[test]
fn search_index_scoped_rebuild_prunes_only_selected_providers() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    index.build_index(&sessions, &providers, &tx).unwrap();
    assert!(
        !index.search("missing semicolon", 10).unwrap().is_empty(),
        "Claude fixture should be indexed before scoped rebuild"
    );
    assert!(
        !index.search("async", 10).unwrap().is_empty(),
        "Gemini fixture should be indexed before scoped rebuild"
    );

    let prune_providers = std::collections::HashSet::from([Provider::ClaudeCode]);
    let stats = index
        .build_index_for_providers(&[], &providers, &tx, &prune_providers)
        .unwrap();
    assert!(
        stats.removed >= 1,
        "scoped rebuild should remove stale Claude entries"
    );
    assert!(
        index.search("missing semicolon", 10).unwrap().is_empty(),
        "Claude docs should be pruned when Claude is in scope"
    );
    assert!(
        !index.search("async", 10).unwrap().is_empty(),
        "out-of-scope Gemini docs must survive a Claude-only rebuild"
    );
}

#[test]
fn search_index_unscoped_partial_rebuild_prunes_nothing() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    index.build_index(&sessions, &providers, &tx).unwrap();
    let stats = index
        .build_index_without_pruning(&[], &providers, &tx)
        .unwrap();

    assert_eq!(stats.removed, 0);
    assert!(
        !index.search("missing semicolon", 10).unwrap().is_empty(),
        "no-prune partial rebuild must preserve existing Claude docs"
    );
    assert!(
        !index.search("async", 10).unwrap().is_empty(),
        "no-prune partial rebuild must preserve existing Gemini docs"
    );
}

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

    // "build error" appears in msg-001 (user) and msg-002 (assistant) of session-abc123.
    // Tantivy parses multi-word queries as OR, so other sessions with "error" may also match.
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

    // "missing semicolon" appears in msg-004 (thinking block)
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
    // Token only appears inside a tool_result block — proves we index the
    // tool output field and that queries hit it.
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

#[test]
fn search_index_rebuilds_when_schema_changes() {
    use tantivy::schema::{Schema, STORED, STRING};
    use tantivy::Index;

    // Simulate an index built before tool_output existed by writing a
    // dummy index with an older schema, then re-opening through SearchIndex.
    let index_dir = tempfile::tempdir().unwrap();
    {
        let mut builder = Schema::builder();
        builder.add_text_field("session_id", STRING | STORED);
        let schema = builder.build();
        Index::create_in_dir(index_dir.path(), schema).unwrap();
    }

    // open_or_create must detect the schema mismatch and recreate the index
    // rather than panicking or returning a SearchIndex with stale fields.
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    // The newly recreated index should be functional: build + search.
    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }
    let (tx, _rx) = crossbeam_channel::unbounded();
    index.build_index(&sessions, &providers, &tx).unwrap();

    let hits = index.search("build error", 10).unwrap();
    assert!(!hits.is_empty(), "rebuilt index should be queryable");
}

#[test]
fn search_index_schema_reset_refuses_unknown_files() {
    use tantivy::schema::{Schema, STORED, STRING};
    use tantivy::Index;

    let index_dir = tempfile::tempdir().unwrap();
    {
        let mut builder = Schema::builder();
        builder.add_text_field("session_id", STRING | STORED);
        let schema = builder.build();
        Index::create_in_dir(index_dir.path(), schema).unwrap();
    }
    let keep = index_dir.path().join("keep.txt");
    fs::write(&keep, "do not delete").unwrap();

    let Err(err) = SearchIndex::open_or_create(index_dir.path()) else {
        panic!("schema reset should reject dir with unknown files");
    };
    assert!(
        err.to_string()
            .contains("refusing to reset index directory"),
        "unexpected error: {err}"
    );
    assert_eq!(fs::read_to_string(&keep).unwrap(), "do not delete");
}

#[cfg(unix)]
#[test]
fn search_index_schema_reset_refuses_symlink_entries() {
    use std::os::unix::fs::symlink;
    use tantivy::schema::{Schema, STORED, STRING};
    use tantivy::Index;

    let index_dir = tempfile::tempdir().unwrap();
    {
        let mut builder = Schema::builder();
        builder.add_text_field("session_id", STRING | STORED);
        let schema = builder.build();
        Index::create_in_dir(index_dir.path(), schema).unwrap();
    }
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("target.txt");
    fs::write(&target, "do not touch").unwrap();
    symlink(&target, index_dir.path().join("linked-target")).unwrap();

    let Err(err) = SearchIndex::open_or_create(index_dir.path()) else {
        panic!("schema reset should reject symlink entries");
    };
    assert!(
        err.to_string()
            .contains("refusing to reset index directory"),
        "unexpected error: {err}"
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), "do not touch");
}

#[test]
fn search_index_does_not_delete_arbitrary_meta_json() {
    let index_dir = tempfile::tempdir().unwrap();
    let meta = index_dir.path().join("meta.json");
    fs::write(&meta, r#"{"not":"tantivy"}"#).unwrap();

    let Err(err) = SearchIndex::open_or_create(index_dir.path()) else {
        panic!("arbitrary meta.json should not be treated as an aghist cache");
    };
    assert!(
        err.to_string().contains("index error"),
        "unexpected error: {err}"
    );
    assert_eq!(fs::read_to_string(&meta).unwrap(), r#"{"not":"tantivy"}"#);
}

#[test]
fn search_empty_index() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let (tx, _rx) = crossbeam_channel::unbounded();

    // Index with zero sessions
    let stats = index.build_index(&[], &providers, &tx).unwrap();
    assert_eq!(stats.sessions_indexed, 0);
    assert_eq!(stats.messages_indexed, 0);

    // Any query returns empty
    let hits = index.search("build error", 10).unwrap();
    assert!(hits.is_empty(), "empty index should return no hits");

    let hits = index.search("async", 10).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn search_no_results_queries() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    index.build_index(&sessions, &providers, &tx).unwrap();

    // Various non-matching queries
    for query in &["xyznonexistent12345", "quantum_entanglement_flux", "zebra"] {
        let hits = index.search(query, 10).unwrap();
        assert!(hits.is_empty(), "query '{query}' should return no hits");
    }

    // Empty/whitespace queries
    let hits = index.search("", 10).unwrap();
    assert!(hits.is_empty(), "empty query should return no hits");

    let hits = index.search("   ", 10).unwrap();
    assert!(hits.is_empty(), "whitespace query should return no hits");
}

#[test]
fn search_incremental_reindex_after_file_change() {
    let tmp = tempfile::tempdir().unwrap();
    let index_dir = tmp.path().join("index");
    let fixture_dir = tmp.path().join("claude");

    // Copy Claude fixture to a temp directory we can modify
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

    // First build
    let stats1 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert!(stats1.sessions_indexed > 0);
    assert!(stats1.messages_indexed > 0);

    // Verify original content is indexed
    let hits = index.search("build error", 10).unwrap();
    assert!(!hits.is_empty());

    // New content should NOT be found yet
    let hits = index.search("quantum entanglement refactor", 10).unwrap();
    assert!(hits.is_empty());

    // Modify the session file: append a new message with unique content
    let session_file = fixture_dir
        .join("projects")
        .join("test-project")
        .join("session-abc123.jsonl");

    // Ensure mtime actually changes (some filesystems have 1s resolution)
    thread::sleep(Duration::from_millis(1100));

    let new_line = r#"{"type":"user","uuid":"msg-005","timestamp":"2025-04-08T10:01:00Z","message":{"role":"user","content":"Apply the quantum entanglement refactor to the parser"},"cwd":"/home/user/project"}"#;
    let mut content = fs::read_to_string(&session_file).unwrap();
    content.push_str(new_line);
    content.push('\n');
    fs::write(&session_file, content).unwrap();

    // Re-discover sessions (mtime has changed)
    let mut sessions = Vec::new();
    for p in &providers {
        sessions.extend(p.discover_sessions().unwrap());
    }

    // Incremental rebuild should re-index the changed session
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

    // New content should now be searchable
    let hits = index.search("quantum entanglement refactor", 10).unwrap();
    assert!(
        !hits.is_empty(),
        "new content should appear after incremental reindex"
    );
    assert_eq!(hits[0].session_id, "session-abc123");
    assert_eq!(hits[0].message_id, "msg-005");

    // Old content should still be searchable
    let hits = index.search("build error", 10).unwrap();
    assert!(
        !hits.is_empty(),
        "original content should survive incremental reindex"
    );
}
