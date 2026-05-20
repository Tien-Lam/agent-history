use super::*;

#[test]
fn decisions_extracted_and_sorted_by_score() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        2,
    );
    let m = vec![
        assistant("Background context.", ts(2025, 9)),
        assistant("We decided to use BM25 instead of cosine.", ts(2025, 10)),
    ];
    let report = aggregate("alpha", &[(s, m)], ProjectLimits::DEFAULTS);
    assert!(
        !report.decisions.is_empty(),
        "expected at least one decision"
    );
    let d = &report.decisions[0];
    assert_eq!(d.turn, 2);
    assert_eq!(d.session_id, "s");
    assert!(d.reference.contains("claude-code/s#2"));
    assert!(d.score >= 5.0);
}

#[test]
fn todos_extracted_with_citation_refs() {
    let s = mk_session(
        "s",
        Some("alpha"),
        Some("claude-sonnet-4-5"),
        None,
        ts(2025, 9),
        1,
    );
    let m = vec![assistant("TODO: revisit error handling", ts(2025, 9))];
    let report = aggregate("alpha", &[(s, m)], ProjectLimits::DEFAULTS);
    assert!(!report.todos.is_empty());
    assert_eq!(report.todos[0].turn, 1);
    assert!(report.todos[0].reference.contains("#1"));
}
