use super::super::support::*;

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

    let hits = index.search("build error", 10).unwrap();
    assert!(
        !hits.is_empty(),
        "should find 'build error' in Claude fixture"
    );
    assert_eq!(hits[0].session_id, "session-abc123");
    assert!(!hits[0].snippet.is_empty());

    let hits = index.search("async", 10).unwrap();
    assert!(!hits.is_empty(), "should find 'async' in Gemini fixture");

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

    let stats1 = index.build_index(&sessions, &providers, &tx).unwrap();
    assert!(stats1.sessions_indexed > 0);
    assert_eq!(
        stats1.added, stats1.sessions_indexed,
        "first run is all 'added'"
    );
    assert_eq!(stats1.updated, 0);
    assert_eq!(stats1.unchanged, 0);

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

    index.clear().unwrap();
    let hits = index.search("build error", 10).unwrap();
    assert!(hits.is_empty(), "should find nothing after clear");

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
