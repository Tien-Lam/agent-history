use super::*;

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
