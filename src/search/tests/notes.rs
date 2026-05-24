use super::*;
use crate::metadata::Note;
use std::collections::HashSet;

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
    let (_dir, index, _s1, _s2) = build_tiny_index();
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
    assert_eq!(hits[0].kind(), HitKind::Note);
    assert_eq!(hits[0].note_id(), Some(1));
    assert_eq!(hits[0].note_session_ref(), Some("claude-code/sess-1#3"));
}
#[test]
fn index_notes_is_incremental_on_unchanged_updated_at() {
    let (_dir, index, _s1, _s2) = build_tiny_index();
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
    let (_dir, index, _s1, _s2) = build_tiny_index();
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
        old_hits.iter().all(|h| h.kind() != HitKind::Note),
        "old note body should have been replaced: {old_hits:?}"
    );
    // New body must match.
    let new_hits = index
        .search_with_filters("new", 10, &SearchFilters::default())
        .unwrap();
    assert!(new_hits.iter().any(|h| h.kind() == HitKind::Note));
}
#[test]
fn index_notes_prunes_removed_rows() {
    let (_dir, index, _s1, _s2) = build_tiny_index();
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
        hits.iter().all(|h| h.kind() != HitKind::Note),
        "pruned note must not match: {hits:?}"
    );
}

#[test]
fn note_sync_prunes_stale_docs_when_metadata_db_is_missing() {
    let (dir, index, _s1, _s2) = build_tiny_index();
    let notes = vec![make_note(
        11,
        "claude-code/sess-1",
        "stale sidecar note marker11",
        "2026-01-01T00:00:00Z",
    )];
    index.index_notes(&notes).unwrap();
    assert!(
        index
            .search_with_filters("marker11", 10, &SearchFilters::default())
            .unwrap()
            .iter()
            .any(|h| h.kind() == HitKind::Note),
        "test setup should index note before pruning"
    );

    crate::search::service::index_notes_best_effort_for_path(
        &index,
        Some(dir.path().join("missing-metadata.db")),
    );

    let hits = index
        .search_with_filters("marker11", 10, &SearchFilters::default())
        .unwrap();
    assert!(
        hits.iter().all(|h| h.kind() != HitKind::Note),
        "missing metadata sidecar should prune stale note docs: {hits:?}"
    );
}

#[test]
fn note_sync_prunes_notes_outside_current_session_refs() {
    let (dir, index, _s1, _s2) = build_tiny_index();
    let db_path = dir.path().join("metadata.db");
    let conn = crate::metadata::open(&db_path).unwrap();
    crate::metadata::note_add(&conn, "claude-code/sess-1#3", "visiblem13").unwrap();
    crate::metadata::note_add(&conn, "claude-code/missing", "hiddenm13").unwrap();

    crate::search::service::index_notes_best_effort_for_session_refs(
        &index,
        Some(db_path),
        &HashSet::from(["claude-code/sess-1".to_string()]),
    );

    let visible = index
        .search_with_filters("visiblem13", 10, &SearchFilters::default())
        .unwrap();
    assert!(
        visible.iter().any(|h| h.kind() == HitKind::Note),
        "current-session note should remain searchable: {visible:?}"
    );

    let hidden = index
        .search_with_filters("hiddenm13", 10, &SearchFilters::default())
        .unwrap();
    assert!(
        hidden.iter().all(|h| h.kind() != HitKind::Note),
        "note outside current session refs should be pruned: {hidden:?}"
    );
}
