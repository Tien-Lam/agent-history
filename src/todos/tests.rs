use chrono::TimeZone;

use super::scan::SNIPPET_MAX;
use super::*;
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, SessionId};

fn make_msg(text: &str, role: Role) -> Message {
    Message {
        id: MessageId("m".into()),
        role,
        timestamp: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        content: vec![ContentBlock::Text(text.into())],
        model: None,
        token_usage: None,
    }
}

fn extract(text: &str) -> Vec<TodoCandidate> {
    let msg = make_msg(text, Role::Assistant);
    extract_from_messages(
        Provider::ClaudeCode,
        &SessionId("sess".into()),
        std::slice::from_ref(&msg),
        &[],
    )
}

#[test]
fn matches_uppercase_todo_word() {
    let hits = extract("TODO: revisit this");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, TodoKind::Todo);
    assert_eq!(hits[0].snippet, "TODO: revisit this");
}

#[test]
fn skips_lowercase_todo_in_prose() {
    // "todo" inside prose is too noisy to match.
    let hits = extract("I added it to my todo list");
    assert!(hits.is_empty(), "got: {hits:?}");
}

#[test]
fn skips_todowrite_tool_name() {
    // Claude Code transcripts mention this constantly.
    let hits = extract("call the TodoWrite tool to track work");
    assert!(hits.is_empty(), "got: {hits:?}");
}

#[test]
fn matches_follow_up_with_dash_or_space() {
    assert_eq!(extract("Need a follow-up here").len(), 1);
    assert_eq!(extract("Need a Follow Up here").len(), 1);
}

#[test]
fn matches_come_back_to_phrase() {
    let hits = extract("We need to come back to this later");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, TodoKind::ComeBackTo);
}

#[test]
fn matches_we_should_phrase() {
    let hits = extract("we should refactor this module");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, TodoKind::WeShould);
}

#[test]
fn extracts_bd_ref_with_digit_suffix() {
    let hits = extract("blocked on ahist-y3o.7.2 — need follow-up");
    assert!(hits.iter().any(|h| h.kind == TodoKind::BdRef));
    let bd = hits.iter().find(|h| h.kind == TodoKind::BdRef).unwrap();
    assert_eq!(bd.bd_id.as_deref(), Some("ahist-y3o.7.2"));
}

#[test]
fn bd_ref_ignores_prose_hyphenates() {
    // "follow-up" must not match as a bd ref because it has no digits.
    let hits = extract("Need a follow-up but no bd id");
    assert!(
        hits.iter().all(|h| h.kind != TodoKind::BdRef),
        "got: {hits:?}"
    );
}

#[test]
fn bd_ref_strips_trailing_period() {
    let hits = extract("Closed in ahist-7ag.");
    let bd = hits.iter().find(|h| h.kind == TodoKind::BdRef).unwrap();
    assert_eq!(bd.bd_id.as_deref(), Some("ahist-7ag"));
}

#[test]
fn citation_ref_uses_one_based_turn() {
    let msgs = vec![
        make_msg("nothing here", Role::User),
        make_msg("TODO second", Role::Assistant),
    ];
    let hits = extract_from_messages(Provider::ClaudeCode, &SessionId("s".into()), &msgs, &[]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].citation.turn, 2);
}

#[test]
fn kind_filter_respected() {
    let msg = make_msg("TODO and we should ahist-1", Role::Assistant);
    let hits = extract_from_messages(
        Provider::ClaudeCode,
        &SessionId("s".into()),
        std::slice::from_ref(&msg),
        &[TodoKind::BdRef],
    );
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, TodoKind::BdRef);
}

#[test]
fn snippet_truncates_long_lines() {
    let long: String = "x".repeat(SNIPPET_MAX + 50) + " TODO";
    let hits = extract(&long);
    assert_eq!(hits.len(), 1);
    let chars = hits[0].snippet.chars().count();
    assert!(chars <= SNIPPET_MAX, "snippet not truncated: {chars}");
    assert!(hits[0].snippet.ends_with('…'));
}

#[test]
fn slug_round_trip_for_all_kinds() {
    for k in TodoKind::ALL {
        assert_eq!(TodoKind::from_slug(k.slug()), Some(k));
    }
}
