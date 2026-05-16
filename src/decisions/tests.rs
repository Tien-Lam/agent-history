use chrono::TimeZone;

use super::*;
use crate::model::{ContentBlock, Message, MessageId, Role};

fn assistant(text: &str) -> Message {
    Message {
        id: MessageId("m1".into()),
        role: Role::Assistant,
        timestamp: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        content: vec![ContentBlock::Text(text.into())],
        model: None,
        token_usage: None,
    }
}

#[test]
fn explicit_decision_passes_default_threshold() {
    let m = assistant("After discussion we decided to use BM25 for ranking.");
    let candidates = extract_from_message(&m, 7, DEFAULT_THRESHOLD);
    assert_eq!(candidates.len(), 1);
    let c = &candidates[0];
    assert_eq!(c.turn, 7);
    assert!(c.score >= 5.0);
    assert!(c.markers.iter().any(|m| m == "we decided"));
    assert!(c.snippet.contains("BM25"));
}

#[test]
fn comparative_choice_passes() {
    let m = assistant("We will use BM25 instead of cosine similarity.");
    let candidates = extract_from_message(&m, 1, DEFAULT_THRESHOLD);
    assert_eq!(candidates.len(), 1);
    let markers = &candidates[0].markers;
    assert!(markers.iter().any(|m| m == "we will"));
    assert!(markers.iter().any(|m| m == "instead of"));
}

#[test]
fn lone_soft_marker_is_dropped() {
    let m = assistant("It works because of caching.");
    let candidates = extract_from_message(&m, 1, DEFAULT_THRESHOLD);
    assert!(
        candidates.is_empty(),
        "lone 'because' should not pass: {candidates:?}"
    );
}

#[test]
fn code_blocks_are_ignored() {
    let m = Message {
        content: vec![ContentBlock::CodeBlock {
            language: Some("rs".into()),
            code: "// we decided to inline this\nfn main() {}".into(),
        }],
        ..assistant("")
    };
    let candidates = extract_from_message(&m, 1, DEFAULT_THRESHOLD);
    assert!(candidates.is_empty(), "code blocks must not match");
}

#[test]
fn sentence_segmentation_emits_one_per_decision() {
    let m = assistant("Background. We decided to ship v1. Later we will revisit.");
    let candidates = extract_from_message(&m, 1, DEFAULT_THRESHOLD);
    assert_eq!(candidates.len(), 2, "got: {candidates:?}");
    assert!(candidates[0].snippet.starts_with("We decided"));
    assert!(candidates[1].snippet.starts_with("Later we will"));
}

#[test]
fn extract_from_messages_assigns_turn_numbers() {
    let msgs = vec![
        assistant("Nothing here."),
        assistant("We decided to drop the cache."),
        assistant("Filler."),
        assistant("We will use sled instead of rocksdb."),
    ];
    let candidates = extract_from_messages(&msgs, DEFAULT_THRESHOLD);
    let turns: Vec<u32> = candidates.iter().map(|c| c.turn).collect();
    assert_eq!(turns, vec![2, 4]);
}

#[test]
fn snippet_is_clipped_for_pathological_input() {
    let long = "We decided to ".to_string() + &"x".repeat(MAX_SNIPPET_CHARS * 2) + ".";
    let m = assistant(&long);
    let candidates = extract_from_message(&m, 1, DEFAULT_THRESHOLD);
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].snippet.chars().count() <= MAX_SNIPPET_CHARS);
    assert!(candidates[0].snippet.ends_with('\u{2026}'));
}

#[test]
fn case_insensitive_match() {
    let m = assistant("WE DECIDED to ship.");
    let candidates = extract_from_message(&m, 1, DEFAULT_THRESHOLD);
    assert_eq!(candidates.len(), 1);
}

#[test]
fn threshold_is_respected() {
    let m = assistant("Maybe we should reconsider this.");
    // "we should" (2.0) + "should" (1.0) = 3.0, which passes the default.
    let lo = extract_from_message(&m, 1, DEFAULT_THRESHOLD);
    assert_eq!(lo.len(), 1);
    let hi = extract_from_message(&m, 1, 5.0);
    assert!(hi.is_empty());
}
