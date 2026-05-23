use super::*;

#[test]
fn clear_reports_manifest_removal_failure() {
    let dir = tempdir().unwrap();
    let index = SearchIndex::open_or_create(dir.path()).unwrap();
    std::fs::create_dir(dir.path().join("manifest.json")).unwrap();

    let err = index.clear().unwrap_err();

    assert!(matches!(err, SearchError::Io(_)), "{err}");
}

#[test]
fn duplicate_raw_session_ids_do_not_overwrite_each_other() {
    let dir = tempdir().unwrap();
    let index = SearchIndex::open_or_create(dir.path()).unwrap();
    let stub = StubProvider::new(Provider::ClaudeCode);

    let mut first = make_session("shared-id", "alpha");
    first.source_path = dir.path().join("first.jsonl");
    std::fs::write(&first.source_path, "first").unwrap();
    let mut second = make_session("shared-id", "beta");
    second.source_path = dir.path().join("second.jsonl");
    std::fs::write(&second.source_path, "second").unwrap();

    stub.add(
        first.clone(),
        vec![make_message("msg", "alpha unique overwrite guard")],
    );
    stub.add(
        second.clone(),
        vec![make_message("msg", "beta unique overwrite guard")],
    );

    let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
    let sessions = vec![first, second];
    let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
    index.build_index(&sessions, &providers, &tx).unwrap();

    let alpha = index.search("alpha", 10).unwrap();
    let beta = index.search("beta", 10).unwrap();
    assert_eq!(
        alpha.len(),
        1,
        "first duplicate-id session was lost: {alpha:?}"
    );
    assert_eq!(
        beta.len(),
        1,
        "second duplicate-id session was lost: {beta:?}"
    );
    assert_eq!(alpha[0].session_id, "shared-id");
    assert_eq!(beta[0].session_id, "shared-id");
    assert_ne!(alpha[0].session_key, beta[0].session_key);
    assert_ne!(alpha[0].message_key, beta[0].message_key);
}

#[test]
fn sessions_sharing_one_source_path_are_all_indexed() {
    let dir = tempdir().unwrap();
    let index = SearchIndex::open_or_create(dir.path()).unwrap();
    let stub = StubProvider::new(Provider::Cursor);

    let shared_db = dir.path().join("state.vscdb");
    std::fs::write(&shared_db, "cursor db snapshot").unwrap();
    let mut first = make_session("composer-a", "alpha");
    first.provider = Provider::Cursor;
    first.source_path = shared_db.clone();
    let mut second = make_session("composer-b", "beta");
    second.provider = Provider::Cursor;
    second.source_path = shared_db;

    stub.add(
        first.clone(),
        vec![make_message("msg-a", "alpha cursor composer")],
    );
    stub.add(
        second.clone(),
        vec![make_message("msg-b", "beta cursor composer")],
    );

    let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
    let sessions = vec![first, second];
    let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
    index.build_index(&sessions, &providers, &tx).unwrap();

    assert_eq!(index.search("alpha", 10).unwrap().len(), 1);
    assert_eq!(index.search("beta", 10).unwrap().len(), 1);
}

#[test]
fn build_index_reports_session_load_errors() {
    let dir = tempdir().unwrap();
    let index = SearchIndex::open_or_create(dir.path()).unwrap();
    let stub = StubProvider::new(Provider::ClaudeCode);

    let mut bad = make_session("bad-session", "broken");
    bad.source_path = dir.path().join("bad.jsonl");
    std::fs::write(&bad.source_path, "bad").unwrap();
    stub.fail(bad.clone(), "fixture parse exploded");

    let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
    let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
    let stats = index.build_index(&[bad], &providers, &tx).unwrap();

    assert_eq!(stats.sessions_indexed, 0);
    assert_eq!(stats.messages_indexed, 0);
    assert_eq!(stats.load_errors.len(), 1);
    assert_eq!(stats.load_errors[0].provider, Provider::ClaudeCode);
    assert_eq!(stats.load_errors[0].session_id, "bad-session");
    assert!(stats.load_errors[0]
        .error
        .contains("fixture parse exploded"));
}

#[test]
fn build_index_reports_source_fingerprint_errors() {
    let dir = tempdir().unwrap();
    let index = SearchIndex::open_or_create(dir.path()).unwrap();
    let stub = StubProvider::new(Provider::ClaudeCode);

    let mut missing = make_session("missing-source", "broken");
    missing.source_path = dir.path().join("missing.jsonl");
    stub.add(
        missing.clone(),
        vec![make_message("msg", "should never be indexed")],
    );

    let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
    let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
    let stats = index.build_index(&[missing], &providers, &tx).unwrap();

    assert_eq!(stats.sessions_indexed, 0);
    assert_eq!(stats.messages_indexed, 0);
    assert_eq!(stats.load_errors.len(), 1);
    assert_eq!(stats.load_errors[0].provider, Provider::ClaudeCode);
    assert_eq!(stats.load_errors[0].session_id, "missing-source");
    assert!(stats.load_errors[0].error.contains("missing.jsonl"));
    assert!(index.search("should", 10).unwrap().is_empty());
}

#[test]
fn load_error_preserves_existing_indexed_docs() {
    let dir = tempdir().unwrap();
    let index = SearchIndex::open_or_create(dir.path()).unwrap();
    let stub = StubProvider::new(Provider::ClaudeCode);

    let mut session = make_session("kept-session", "stable");
    session.source_path = dir.path().join("kept.jsonl");
    std::fs::write(&session.source_path, "first").unwrap();
    stub.add(
        session.clone(),
        vec![make_message("msg", "durable indexed content")],
    );

    let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
    let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
    index
        .build_index(&[session.clone()], &providers, &tx)
        .unwrap();
    assert_eq!(index.search("durable", 10).unwrap().len(), 1);

    std::fs::write(&session.source_path, "second").unwrap();
    let failing_stub = StubProvider::new(Provider::ClaudeCode);
    failing_stub.fail(session.clone(), "second load failed");
    let failing_providers: Vec<Box<dyn crate::provider::HistoryProvider>> =
        vec![Box::new(failing_stub)];

    let stats = index
        .build_index(&[session], &failing_providers, &tx)
        .unwrap();

    assert_eq!(stats.updated, 0);
    assert_eq!(stats.sessions_indexed, 0);
    assert_eq!(stats.load_errors.len(), 1);
    assert_eq!(
        index.search("durable", 10).unwrap().len(),
        1,
        "last good docs should remain searchable when reload fails"
    );
}

#[test]
fn build_index_prunes_sessions_no_longer_discovered() {
    let dir = tempdir().unwrap();
    let index = SearchIndex::open_or_create(dir.path()).unwrap();
    let stub = StubProvider::new(Provider::ClaudeCode);

    let mut first = make_session("sess-a", "alpha");
    first.source_path = dir.path().join("a.jsonl");
    std::fs::write(&first.source_path, "a").unwrap();
    let mut second = make_session("sess-b", "beta");
    second.source_path = dir.path().join("b.jsonl");
    std::fs::write(&second.source_path, "b").unwrap();

    stub.add(first.clone(), vec![make_message("a", "alpha survives")]);
    stub.add(second.clone(), vec![make_message("b", "beta removed")]);

    let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
    let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
    index
        .build_index(&[first.clone(), second], &providers, &tx)
        .unwrap();

    let stats = index.build_index(&[first], &providers, &tx).unwrap();
    assert_eq!(stats.removed, 1);
    assert_eq!(index.search("alpha", 10).unwrap().len(), 1);
    assert!(index.search("beta", 10).unwrap().is_empty());
}

#[test]
fn file_fingerprint_includes_content_hash() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    std::fs::write(&path, "first").unwrap();
    let first = file_fingerprint(&path).unwrap();
    std::fs::write(&path, "second").unwrap();
    let second = file_fingerprint(&path).unwrap();

    assert_ne!(first.sha256, second.sha256);
}
