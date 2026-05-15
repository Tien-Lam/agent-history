use rusqlite::Connection;
use tempfile::TempDir;

use super::connection::migrations;
use super::*;

#[test]
fn migrations_validate() {
    migrations().validate().expect("migrations are valid");
}

#[test]
fn open_creates_db_and_parent_dir() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("nested/dir/metadata.db");
    let conn = open(&path).unwrap();
    assert!(path.exists());
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .filter_map(std::result::Result::ok)
        .filter(|n| !n.starts_with("sqlite_"))
        .collect();
    assert!(tables.contains(&"notes".to_string()), "tables = {tables:?}");
    assert!(tables.contains(&"tags".to_string()), "tables = {tables:?}");
    assert!(tables.contains(&"stars".to_string()), "tables = {tables:?}");
}

#[test]
fn open_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("metadata.db");
    let _ = open(&path).unwrap();
    let conn = open(&path).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert!(version >= 1, "user_version should be set after migration");
}

#[test]
fn schema_supports_basic_inserts() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("metadata.db");
    let conn = open(&path).unwrap();
    conn.execute(
        "INSERT INTO notes(session_ref, body) VALUES (?1, ?2)",
        ("claude-code/abc123#42", "first note"),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
        ("claude-code/abc123", "review"),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO stars(session_ref) VALUES (?1)",
        ["claude-code/abc123"],
    )
    .unwrap();

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn stars_are_unique_per_session_ref() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("metadata.db");
    let conn = open(&path).unwrap();
    conn.execute(
        "INSERT INTO stars(session_ref) VALUES (?1)",
        ["claude-code/abc"],
    )
    .unwrap();
    let dup = conn.execute(
        "INSERT INTO stars(session_ref) VALUES (?1)",
        ["claude-code/abc"],
    );
    assert!(dup.is_err(), "duplicate star should violate UNIQUE");
}

#[test]
fn tags_are_unique_per_session_ref_tag_pair() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("metadata.db");
    let conn = open(&path).unwrap();
    conn.execute(
        "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
        ("claude-code/abc", "review"),
    )
    .unwrap();
    let dup = conn.execute(
        "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
        ("claude-code/abc", "review"),
    );
    assert!(
        dup.is_err(),
        "duplicate (session_ref,tag) should violate UNIQUE"
    );

    // Same session_ref with a different tag is allowed.
    conn.execute(
        "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
        ("claude-code/abc", "todo"),
    )
    .unwrap();
}

fn open_fresh() -> (TempDir, Connection) {
    let tmp = TempDir::new().unwrap();
    let conn = open(&tmp.path().join("metadata.db")).unwrap();
    (tmp, conn)
}

#[test]
fn validate_accepts_session_and_turn_refs() {
    validate_session_ref("claude-code/abc-123").unwrap();
    validate_session_ref("claude-code/abc-123#7").unwrap();
    validate_session_ref("opencode/ses_xyz#99").unwrap();
}

#[test]
fn validate_rejects_bad_refs() {
    assert!(matches!(
        validate_session_ref(""),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("no-slash"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("Claude-Code/abc"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("claude-code/"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("claude-code/abc#0"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        validate_session_ref("claude-code/abc#two"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
}

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

#[test]
fn tag_add_returns_populated_row() {
    let (_tmp, conn) = open_fresh();
    let tag = tag_add(&conn, "claude-code/abc", "review").unwrap();
    assert!(tag.id >= 1);
    assert_eq!(tag.session_ref, "claude-code/abc");
    assert_eq!(tag.tag, "review");
    assert!(!tag.created_at.is_empty());
}

#[test]
fn tag_add_trims_and_rejects_empty() {
    let (_tmp, conn) = open_fresh();
    let tag = tag_add(&conn, "claude-code/abc", "  todo  ").unwrap();
    assert_eq!(tag.tag, "todo");
    assert!(matches!(
        tag_add(&conn, "claude-code/abc", "   "),
        Err(MetadataError::EmptyTag)
    ));
}

#[test]
fn tag_add_rejects_duplicate() {
    let (_tmp, conn) = open_fresh();
    tag_add(&conn, "claude-code/abc", "review").unwrap();
    let err = tag_add(&conn, "claude-code/abc", "review");
    assert!(matches!(
        err,
        Err(MetadataError::TagAlreadyExists { ref session_ref, ref tag })
            if session_ref == "claude-code/abc" && tag == "review"
    ));
}

#[test]
fn tag_add_rejects_invalid_ref() {
    let (_tmp, conn) = open_fresh();
    assert!(matches!(
        tag_add(&conn, "bad-provider/abc", "review"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
}

#[test]
fn tag_list_filters_by_session_or_turn_or_tag_value() {
    let (_tmp, conn) = open_fresh();
    let session = tag_add(&conn, "claude-code/abc", "review").unwrap();
    let turn7 = tag_add(&conn, "claude-code/abc#7", "todo").unwrap();
    let turn7_review = tag_add(&conn, "claude-code/abc#7", "review").unwrap();
    let other = tag_add(&conn, "opencode/xyz", "review").unwrap();

    let all = tag_list(&conn, None, None).unwrap();
    assert_eq!(all.len(), 4);

    // Session-level filter sees the session row + every turn under it.
    let scoped = tag_list(&conn, Some("claude-code/abc"), None).unwrap();
    let ids: Vec<_> = scoped.iter().map(|t| t.id).collect();
    assert!(ids.contains(&session.id));
    assert!(ids.contains(&turn7.id));
    assert!(ids.contains(&turn7_review.id));
    assert!(!ids.contains(&other.id));

    // Turn-level filter is exact.
    let turn_only = tag_list(&conn, Some("claude-code/abc#7"), None).unwrap();
    let ids: Vec<_> = turn_only.iter().map(|t| t.id).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&turn7.id));
    assert!(ids.contains(&turn7_review.id));

    // Tag-value filter narrows across sessions.
    let reviews = tag_list(&conn, None, Some("review")).unwrap();
    assert_eq!(reviews.len(), 3);

    // Combined filters AND together.
    let scoped_review = tag_list(&conn, Some("claude-code/abc"), Some("review")).unwrap();
    let ids: Vec<_> = scoped_review.iter().map(|t| t.id).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&session.id));
    assert!(ids.contains(&turn7_review.id));
}

#[test]
fn tag_remove_returns_deleted_row_and_is_idempotent_negative() {
    let (_tmp, conn) = open_fresh();
    let added = tag_add(&conn, "claude-code/abc", "review").unwrap();
    let removed = tag_remove(&conn, "claude-code/abc", "review").unwrap();
    assert_eq!(removed, added);

    // Removing the same pair again yields TagNotFound.
    assert!(matches!(
        tag_remove(&conn, "claude-code/abc", "review"),
        Err(MetadataError::TagNotFound { .. })
    ));
}

#[test]
fn tag_remove_rejects_invalid_ref_and_empty_tag() {
    let (_tmp, conn) = open_fresh();
    assert!(matches!(
        tag_remove(&conn, "bad/abc", "review"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
    assert!(matches!(
        tag_remove(&conn, "claude-code/abc", "  "),
        Err(MetadataError::EmptyTag)
    ));
}

#[test]
fn star_add_returns_populated_row() {
    let (_tmp, conn) = open_fresh();
    let star = star_add(&conn, "claude-code/abc-123").unwrap();
    assert_eq!(star.session_ref, "claude-code/abc-123");
    assert!(!star.starred_at.is_empty());
}

#[test]
fn star_add_rejects_duplicate() {
    let (_tmp, conn) = open_fresh();
    star_add(&conn, "claude-code/abc").unwrap();
    let err = star_add(&conn, "claude-code/abc");
    assert!(matches!(
        err,
        Err(MetadataError::StarAlreadyExists { ref session_ref })
            if session_ref == "claude-code/abc"
    ));
}

#[test]
fn star_add_rejects_invalid_ref() {
    let (_tmp, conn) = open_fresh();
    assert!(matches!(
        star_add(&conn, "bad-provider/abc"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
}

#[test]
fn star_list_filters_by_session_or_turn() {
    let (_tmp, conn) = open_fresh();
    let session = star_add(&conn, "claude-code/abc").unwrap();
    let turn7 = star_add(&conn, "claude-code/abc#7").unwrap();
    let other = star_add(&conn, "opencode/xyz").unwrap();

    let all = star_list(&conn, None).unwrap();
    assert_eq!(all.len(), 3);

    let scoped = star_list(&conn, Some("claude-code/abc")).unwrap();
    let refs: Vec<_> = scoped.iter().map(|s| s.session_ref.clone()).collect();
    assert!(refs.contains(&session.session_ref));
    assert!(refs.contains(&turn7.session_ref));
    assert!(!refs.contains(&other.session_ref));

    let turn_only = star_list(&conn, Some("claude-code/abc#7")).unwrap();
    assert_eq!(turn_only.len(), 1);
    assert_eq!(turn_only[0].session_ref, "claude-code/abc#7");
}

#[test]
fn star_remove_returns_deleted_row_and_is_idempotent_negative() {
    let (_tmp, conn) = open_fresh();
    let added = star_add(&conn, "claude-code/abc").unwrap();
    let removed = star_remove(&conn, "claude-code/abc").unwrap();
    assert_eq!(removed, added);
    assert!(star_get(&conn, "claude-code/abc").unwrap().is_none());

    assert!(matches!(
        star_remove(&conn, "claude-code/abc"),
        Err(MetadataError::StarNotFound { .. })
    ));
}

#[test]
fn star_remove_rejects_invalid_ref() {
    let (_tmp, conn) = open_fresh();
    assert!(matches!(
        star_remove(&conn, "bad/abc"),
        Err(MetadataError::InvalidSessionRef(_, _))
    ));
}

#[test]
fn env_var_overrides_default_path() {
    let tmp = TempDir::new().unwrap();
    let custom = tmp.path().join("custom.db");
    // SAFETY: tests run sequentially within a test binary by default; the
    // env var is set and read here only.
    std::env::set_var(ENV_PATH, &custom);
    let resolved = default_path().expect("path resolves with env var set");
    std::env::remove_var(ENV_PATH);
    assert_eq!(resolved, custom);
}

#[test]
fn filter_session_keys_returns_none_when_no_filter_active() {
    let (_tmp, conn) = open_fresh();
    let result = filter_session_keys(&conn, None, None, false).unwrap();
    assert!(result.is_none());
}

#[test]
fn filter_session_keys_strips_turn_suffix() {
    let (_tmp, conn) = open_fresh();
    star_add(&conn, "claude-code/abc#3").unwrap();
    let keys = filter_session_keys(&conn, None, None, true)
        .unwrap()
        .unwrap();
    assert_eq!(keys.len(), 1);
    assert!(keys.contains("claude-code/abc"));
}

#[test]
fn filter_session_keys_note_substring_is_case_insensitive() {
    let (_tmp, conn) = open_fresh();
    note_add(&conn, "claude-code/sess-a", "Look at THIS bug").unwrap();
    note_add(&conn, "claude-code/sess-b#2", "unrelated text").unwrap();
    let keys = filter_session_keys(&conn, Some("this bug"), None, false)
        .unwrap()
        .unwrap();
    assert_eq!(keys.len(), 1);
    assert!(keys.contains("claude-code/sess-a"));
}

#[test]
fn filter_session_keys_empty_note_substring_is_usage_error() {
    let (_tmp, conn) = open_fresh();
    let err = filter_session_keys(&conn, Some("   "), None, false).unwrap_err();
    assert!(matches!(err, MetadataError::EmptyBody));
}

#[test]
fn filter_session_keys_tag_matches_exactly() {
    let (_tmp, conn) = open_fresh();
    tag_add(&conn, "claude-code/sess-a", "review").unwrap();
    tag_add(&conn, "claude-code/sess-b", "todo").unwrap();
    let keys = filter_session_keys(&conn, None, Some("review"), false)
        .unwrap()
        .unwrap();
    assert_eq!(keys.len(), 1);
    assert!(keys.contains("claude-code/sess-a"));
}

#[test]
fn filter_session_keys_combines_with_intersection() {
    let (_tmp, conn) = open_fresh();
    // Two sessions tagged "review"; only one starred. AND across filters
    // keeps the intersection.
    tag_add(&conn, "claude-code/sess-a", "review").unwrap();
    tag_add(&conn, "claude-code/sess-b", "review").unwrap();
    star_add(&conn, "claude-code/sess-a#5").unwrap();
    let keys = filter_session_keys(&conn, None, Some("review"), true)
        .unwrap()
        .unwrap();
    assert_eq!(keys.len(), 1);
    assert!(keys.contains("claude-code/sess-a"));
}

#[test]
fn filter_session_keys_returns_empty_when_no_match() {
    let (_tmp, conn) = open_fresh();
    let keys = filter_session_keys(&conn, None, Some("review"), false)
        .unwrap()
        .unwrap();
    assert!(keys.is_empty());
}
