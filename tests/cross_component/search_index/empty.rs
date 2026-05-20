use super::super::support::*;

#[test]
fn search_empty_index() {
    let index_dir = tempfile::tempdir().unwrap();
    let index = SearchIndex::open_or_create(index_dir.path()).unwrap();

    let providers = all_providers();
    let (tx, _rx) = crossbeam_channel::unbounded();

    let stats = index.build_index(&[], &providers, &tx).unwrap();
    assert_eq!(stats.sessions_indexed, 0);
    assert_eq!(stats.messages_indexed, 0);

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

    for query in &["xyznonexistent12345", "quantum_entanglement_flux", "zebra"] {
        let hits = index.search(query, 10).unwrap();
        assert!(hits.is_empty(), "query '{query}' should return no hits");
    }

    let hits = index.search("", 10).unwrap();
    assert!(hits.is_empty(), "empty query should return no hits");

    let hits = index.search("   ", 10).unwrap();
    assert!(hits.is_empty(), "whitespace query should return no hits");
}
