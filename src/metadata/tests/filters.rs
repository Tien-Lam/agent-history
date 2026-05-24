use super::*;

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
    star_add(&conn, "laptop:claude-code/abc#4").unwrap();
    let keys = filter_session_keys(&conn, None, None, true)
        .unwrap()
        .unwrap();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains("claude-code/abc"));
    assert!(keys.contains("laptop:claude-code/abc"));
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
