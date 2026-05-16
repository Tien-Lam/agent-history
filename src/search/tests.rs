use crate::action::Action;
use crate::metadata::Note;
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};

use super::fingerprint::file_fingerprint;
use super::*;

use chrono::TimeZone;
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::tempdir;

/// A `HistoryProvider` that hands back a canned message list per session.
/// Lets unit tests build a Tantivy index without going through real disk
/// fixtures.
struct StubProvider {
    provider: Provider,
    // Mutex so the trait's `&self` sig can hand out cloned messages without
    // the caller noticing — we don't actually share across threads in tests.
    sessions: Mutex<Vec<Session>>,
    messages: Mutex<std::collections::HashMap<String, Vec<Message>>>,
    base: Vec<PathBuf>,
}

impl StubProvider {
    fn new(provider: Provider) -> Self {
        Self {
            provider,
            sessions: Mutex::new(Vec::new()),
            messages: Mutex::new(std::collections::HashMap::new()),
            base: Vec::new(),
        }
    }

    fn add(&self, session: Session, messages: Vec<Message>) {
        self.messages
            .lock()
            .unwrap()
            .insert(session.identity_key(), messages);
        self.sessions.lock().unwrap().push(session);
    }
}

impl crate::provider::HistoryProvider for StubProvider {
    fn provider(&self) -> Provider {
        self.provider
    }
    fn base_dirs(&self) -> &[PathBuf] {
        &self.base
    }
    fn discover_sessions(&self) -> Result<Vec<Session>, crate::provider::ProviderError> {
        Ok(self.sessions.lock().unwrap().clone())
    }
    fn load_messages(
        &self,
        session: &Session,
    ) -> Result<Vec<Message>, crate::provider::ProviderError> {
        Ok(self
            .messages
            .lock()
            .unwrap()
            .get(&session.identity_key())
            .cloned()
            .unwrap_or_default())
    }
}

fn make_session(id: &str, project: &str) -> Session {
    Session {
        id: SessionId(id.to_string()),
        provider: Provider::ClaudeCode,
        project_path: Some(PathBuf::from(format!("/proj/{project}"))),
        project_name: Some(project.to_string()),
        git_branch: None,
        started_at: chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        ended_at: None,
        summary: None,
        model: None,
        token_usage: None,
        message_count: 0,
        // source_path mtime is read for the manifest; using a real (but
        // empty) tempfile path keeps `file_mtime` happy without crashing.
        source_path: PathBuf::from(format!("/tmp/aghist-stub-{id}")),
    }
}

fn make_message(id: &str, text: &str) -> Message {
    Message {
        id: MessageId(id.to_string()),
        role: Role::User,
        timestamp: chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        content: vec![ContentBlock::Text(text.to_string())],
        model: None,
        token_usage: None,
    }
}

/// Builds a tiny Tantivy index with two sessions / four messages and
/// returns the open `SearchIndex`. Used to exercise hybrid scoring against
/// a real index without depending on filesystem fixtures.
fn build_tiny_index() -> (tempfile::TempDir, SearchIndex) {
    let dir = tempdir().unwrap();
    let index = SearchIndex::open_or_create(dir.path()).unwrap();

    let stub = StubProvider::new(Provider::ClaudeCode);
    let s1 = make_session("sess-1", "alpha");
    let s2 = make_session("sess-2", "beta");
    stub.add(
        s1.clone(),
        vec![
            make_message("m-1", "rust async tokio runtime executor"),
            make_message("m-2", "ratatui terminal user interface"),
        ],
    );
    stub.add(
        s2.clone(),
        vec![
            make_message("m-3", "tantivy full text search engine"),
            make_message("m-4", "fastembed semantic vectors"),
        ],
    );

    let providers: Vec<Box<dyn crate::provider::HistoryProvider>> = vec![Box::new(stub)];
    let sessions: Vec<Session> = vec![s1, s2];
    let (tx, _rx) = crossbeam_channel::unbounded::<Action>();
    index.build_index(&sessions, &providers, &tx).unwrap();

    (dir, index)
}

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
    let first = file_fingerprint(&path);
    std::fs::write(&path, "second").unwrap();
    let second = file_fingerprint(&path);

    assert_ne!(first.sha256, second.sha256);
}

#[test]
fn session_ids_with_messages_filters_by_role() {
    let dir = tempdir().unwrap();
    let index = SearchIndex::open_or_create(dir.path()).unwrap();

    let stub = StubProvider::new(Provider::ClaudeCode);
    let s_user = make_session("sess-user-only", "alpha");
    let s_mixed = make_session("sess-mixed", "beta");
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
    let s_plain = make_session("sess-plain", "alpha");
    let s_with_tool = make_session("sess-with-tool", "beta");
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

#[test]
fn cosine_similarity_handles_identical_orthogonal_and_zero_vectors() {
    let a = [1.0_f32, 0.0, 0.0];
    let b = [1.0_f32, 0.0, 0.0];
    let c = [0.0_f32, 1.0, 0.0];
    let z = [0.0_f32, 0.0, 0.0];

    // Identical → 1.0 (within float tolerance).
    assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    // Orthogonal → 0.0.
    assert!(cosine_similarity(&a, &c).abs() < 1e-6);
    // Zero norm on either side → exactly 0.0 (the function returns 0.0
    // literally, not via float arithmetic, so equality is safe).
    assert!(cosine_similarity(&a, &z).abs() < 1e-6);
    assert!(cosine_similarity(&z, &z).abs() < 1e-6);
    // Mismatched lengths or empty inputs → exactly 0.0 (early return).
    assert!(cosine_similarity(&a, &[1.0_f32, 0.0]).abs() < 1e-6);
    assert!(cosine_similarity(&[][..], &[][..]).abs() < 1e-6);
}

#[test]
fn search_hybrid_falls_back_to_lexical_when_weight_is_zero() {
    let (_dir, index) = build_tiny_index();
    let lex = index
        .search_with_filters("tantivy", 10, &SearchFilters::default())
        .unwrap();
    let hybrid = index
        .search_hybrid(
            "tantivy",
            &[SemanticCandidate {
                message_key: make_session("sess-2", "beta").message_key(1, "m-4"),
                message_id: "m-4".to_string(),
                similarity: 0.9,
            }],
            10,
            &SearchFilters::default(),
            0.0,
            50,
        )
        .unwrap();
    // weight=0 must yield exactly the lexical result, including identical
    // BM25 scores — not RRF scores.
    assert_eq!(hybrid.len(), lex.len());
    for (a, b) in hybrid.iter().zip(lex.iter()) {
        assert_eq!(a.message_id, b.message_id);
        assert!((a.score - b.score).abs() < 1e-6);
    }
}

#[test]
fn search_hybrid_falls_back_to_lexical_when_semantic_pool_is_empty() {
    let (_dir, index) = build_tiny_index();
    let lex = index
        .search_with_filters("tantivy", 10, &SearchFilters::default())
        .unwrap();
    let hybrid = index
        .search_hybrid("tantivy", &[], 10, &SearchFilters::default(), 0.5, 50)
        .unwrap();
    assert_eq!(hybrid.len(), lex.len());
    for (a, b) in hybrid.iter().zip(lex.iter()) {
        assert_eq!(a.message_id, b.message_id);
    }
}

#[test]
fn search_hybrid_promotes_semantic_only_hits() {
    // "tantivy" only matches m-3 lexically. If we pretend the embedding
    // model thinks m-4 ("fastembed semantic vectors") is the top semantic
    // match, RRF should return BOTH — proving hybrid surfaces hits the
    // lexical pass alone wouldn't.
    let (_dir, index) = build_tiny_index();
    let semantic = vec![
        SemanticCandidate {
            message_key: make_session("sess-2", "beta").message_key(1, "m-4"),
            message_id: "m-4".to_string(),
            similarity: 0.95,
        },
        SemanticCandidate {
            message_key: make_session("sess-2", "beta").message_key(0, "m-3"),
            message_id: "m-3".to_string(),
            similarity: 0.80,
        },
    ];
    let hybrid = index
        .search_hybrid("tantivy", &semantic, 10, &SearchFilters::default(), 0.5, 50)
        .unwrap();
    let ids: Vec<&str> = hybrid.iter().map(|h| h.message_id.as_str()).collect();
    assert!(ids.contains(&"m-3"), "lexical hit must survive: {ids:?}");
    assert!(
        ids.contains(&"m-4"),
        "semantic-only hit must surface: {ids:?}"
    );
    // m-3 appears in both pools → its fused score should beat m-4 (which
    // is semantic-only) when weight=0.5.
    let m3_score = hybrid.iter().find(|h| h.message_id == "m-3").unwrap().score;
    let m4_score = hybrid.iter().find(|h| h.message_id == "m-4").unwrap().score;
    assert!(
            m3_score > m4_score,
            "lexical+semantic hit (m-3, score {m3_score}) should outrank semantic-only (m-4, score {m4_score})"
        );
}

fn make_note(id: i64, session_ref: &str, body: &str, updated_at: &str) -> Note {
    Note {
        id,
        session_ref: session_ref.to_string(),
        body: body.to_string(),
        created_at: updated_at.to_string(),
        updated_at: updated_at.to_string(),
    }
}

#[test]
fn index_notes_makes_bodies_searchable_with_kind_note() {
    let (_dir, index) = build_tiny_index();
    let notes = vec![make_note(
        1,
        "claude-code/sess-1#3",
        "investigate xylophone bug",
        "2026-01-01T00:00:00Z",
    )];
    let stats = index.index_notes(&notes).unwrap();
    assert_eq!(stats.added, 1);
    assert_eq!(stats.unchanged, 0);

    let hits = index
        .search_with_filters("xylophone", 10, &SearchFilters::default())
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, HitKind::Note);
    assert_eq!(hits[0].note_id, Some(1));
    assert_eq!(
        hits[0].note_session_ref.as_deref(),
        Some("claude-code/sess-1#3")
    );
}

#[test]
fn index_notes_is_incremental_on_unchanged_updated_at() {
    let (_dir, index) = build_tiny_index();
    let notes = vec![make_note(
        42,
        "claude-code/sess-1",
        "first version",
        "2026-01-01T00:00:00Z",
    )];
    let s1 = index.index_notes(&notes).unwrap();
    assert_eq!(s1.added, 1);
    let s2 = index.index_notes(&notes).unwrap();
    // Re-indexing with the same updated_at must skip everything.
    assert_eq!(s2.added, 0);
    assert_eq!(s2.updated, 0);
    assert_eq!(s2.unchanged, 1);
}

#[test]
fn index_notes_replaces_doc_when_updated_at_advances() {
    let (_dir, index) = build_tiny_index();
    let v1 = vec![make_note(
        7,
        "claude-code/sess-1",
        "old text marker7",
        "2026-01-01T00:00:00Z",
    )];
    index.index_notes(&v1).unwrap();
    let v2 = vec![make_note(
        7,
        "claude-code/sess-1",
        "new text marker7",
        "2026-02-01T00:00:00Z",
    )];
    let stats = index.index_notes(&v2).unwrap();
    assert_eq!(stats.updated, 1);

    // Old body must no longer match.
    let old_hits = index
        .search_with_filters("old", 10, &SearchFilters::default())
        .unwrap();
    assert!(
        old_hits.iter().all(|h| h.kind != HitKind::Note),
        "old note body should have been replaced: {old_hits:?}"
    );
    // New body must match.
    let new_hits = index
        .search_with_filters("new", 10, &SearchFilters::default())
        .unwrap();
    assert!(new_hits.iter().any(|h| h.kind == HitKind::Note));
}

#[test]
fn index_notes_prunes_removed_rows() {
    let (_dir, index) = build_tiny_index();
    let v1 = vec![make_note(
        9,
        "claude-code/sess-1",
        "soon-to-vanish marker9",
        "2026-01-01T00:00:00Z",
    )];
    index.index_notes(&v1).unwrap();
    let stats = index.index_notes(&[]).unwrap();
    assert_eq!(stats.removed, 1);
    let hits = index
        .search_with_filters("soon-to-vanish", 10, &SearchFilters::default())
        .unwrap();
    assert!(
        hits.iter().all(|h| h.kind != HitKind::Note),
        "pruned note must not match: {hits:?}"
    );
}

#[test]
fn search_hybrid_applies_filters_to_semantic_candidates() {
    // Filter to a project that only contains sess-1, but feed in semantic
    // candidates that include m-4 (in sess-2). Hybrid must drop m-4 — the
    // filter applies symmetrically to both ranking sources.
    let (_dir, index) = build_tiny_index();
    let filters = SearchFilters {
        project: Some("alpha".to_string()),
        ..SearchFilters::default()
    };
    let semantic = vec![
        SemanticCandidate {
            message_key: make_session("sess-2", "beta").message_key(1, "m-4"),
            message_id: "m-4".to_string(),
            similarity: 0.95,
        },
        SemanticCandidate {
            message_key: make_session("sess-1", "alpha").message_key(0, "m-1"),
            message_id: "m-1".to_string(),
            similarity: 0.70,
        },
    ];
    let hybrid = index
        .search_hybrid("rust", &semantic, 10, &filters, 0.5, 50)
        .unwrap();
    let ids: Vec<&str> = hybrid.iter().map(|h| h.message_id.as_str()).collect();
    assert!(
        !ids.contains(&"m-4"),
        "project filter must drop m-4 from semantic pool: {ids:?}"
    );
    assert!(
        ids.contains(&"m-1"),
        "filter-passing hit must remain: {ids:?}"
    );
}
