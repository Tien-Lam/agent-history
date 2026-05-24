use super::*;

#[test]
fn note_add_returns_populated_row() {
    let (_tmp, conn) = open_fresh();
    let note = note_add(&conn, "claude-code/abc-123#7", "first note body").unwrap();
    assert!(note.id >= 1);
    assert_eq!(note.session_ref, "claude-code/abc-123#7");
    assert_eq!(note.body, "first note body");
    assert!(!note.created_at.is_empty());
    assert_eq!(note.created_at, note.updated_at);
}

#[test]
fn filter_session_keys_ignores_invalid_stored_refs() {
    let (_tmp, conn) = open_fresh();
    conn.execute(
        "INSERT INTO notes(session_ref, body) VALUES (?1, ?2)",
        ("not-a-session-ref", "needle"),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO notes(session_ref, body) VALUES (?1, ?2)",
        ("claude-code/valid#2", "needle"),
    )
    .unwrap();

    let keys = filter_session_keys(&conn, Some("needle"), None, false)
        .unwrap()
        .unwrap();
    assert_eq!(
        keys,
        ["claude-code/valid".to_string()].into_iter().collect()
    );
}

#[test]
fn note_add_rejects_invalid_ref_and_empty_body() {
    let (_tmp, conn) = open_fresh();
    assert!(matches!(
        note_add(&conn, "bad-provider/abc", "body"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        note_add(&conn, "claude-code/abc", "   \n  "),
        Err(MetadataError::EmptyBody)
    ));
}

#[test]
fn note_add_trims_body() {
    let (_tmp, conn) = open_fresh();
    let note = note_add(&conn, "claude-code/abc", "  hello  \n").unwrap();
    assert_eq!(note.body, "hello");
}

#[test]
fn note_list_filters_by_session_or_turn() {
    let (_tmp, conn) = open_fresh();
    let session = note_add(&conn, "claude-code/abc", "session-level").unwrap();
    let turn7 = note_add(&conn, "claude-code/abc#7", "turn 7").unwrap();
    let turn9 = note_add(&conn, "claude-code/abc#9", "turn 9").unwrap();
    let other = note_add(&conn, "opencode/xyz", "different session").unwrap();

    let all = note_list(&conn, None).unwrap();
    assert_eq!(all.len(), 4);

    // Session-level filter sees the session row + every turn under it,
    // but not unrelated sessions.
    let scoped = note_list(&conn, Some("claude-code/abc")).unwrap();
    let ids: Vec<_> = scoped.iter().map(|n| n.id).collect();
    assert!(ids.contains(&session.id));
    assert!(ids.contains(&turn7.id));
    assert!(ids.contains(&turn9.id));
    assert!(!ids.contains(&other.id));

    // Turn-level filter is exact: only that turn, not the parent session.
    let turn_only = note_list(&conn, Some("claude-code/abc#7")).unwrap();
    assert_eq!(turn_only.len(), 1);
    assert_eq!(turn_only[0].id, turn7.id);
}

#[test]
fn note_list_escapes_like_wildcards_in_session_ids() {
    let (_tmp, conn) = open_fresh();
    let exact = note_add(&conn, "claude-code/a_b%z", "session-level").unwrap();
    let turn = note_add(&conn, "claude-code/a_b%z#1", "turn").unwrap();
    let wildcard_collision = note_add(&conn, "claude-code/axbzz#1", "other").unwrap();

    let scoped = note_list(&conn, Some("claude-code/a_b%z")).unwrap();
    let ids: Vec<_> = scoped.iter().map(|n| n.id).collect();

    assert!(ids.contains(&exact.id));
    assert!(ids.contains(&turn.id));
    assert!(!ids.contains(&wildcard_collision.id));
}

#[test]
fn note_edit_updates_body_and_bumps_timestamp() {
    let (_tmp, conn) = open_fresh();
    let original = note_add(&conn, "claude-code/abc", "v1").unwrap();
    // Force a measurable gap so updated_at moves even on fast machines.
    std::thread::sleep(std::time::Duration::from_millis(10));
    let updated = note_edit(&conn, original.id, "v2").unwrap();
    assert_eq!(updated.id, original.id);
    assert_eq!(updated.body, "v2");
    assert_eq!(updated.created_at, original.created_at);
    assert!(
        updated.updated_at >= original.updated_at,
        "updated_at should advance: {} -> {}",
        original.updated_at,
        updated.updated_at
    );
}

#[test]
fn note_edit_rejects_missing_id_and_empty_body() {
    let (_tmp, conn) = open_fresh();
    let note = note_add(&conn, "claude-code/abc", "v1").unwrap();
    assert!(matches!(
        note_edit(&conn, 9999, "v2"),
        Err(MetadataError::NoteNotFound(9999))
    ));
    assert!(matches!(
        note_edit(&conn, note.id, "  "),
        Err(MetadataError::EmptyBody)
    ));
}

#[test]
fn note_remove_returns_deleted_row_and_is_idempotent_negative() {
    let (_tmp, conn) = open_fresh();
    let note = note_add(&conn, "claude-code/abc", "to remove").unwrap();
    let removed = note_remove(&conn, note.id).unwrap();
    assert_eq!(removed, note);
    assert!(note_get(&conn, note.id).unwrap().is_none());
    assert!(matches!(
        note_remove(&conn, note.id),
        Err(MetadataError::NoteNotFound(_))
    ));
}
