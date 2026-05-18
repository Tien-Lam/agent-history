use super::*;

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
