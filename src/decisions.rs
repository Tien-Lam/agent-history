//! Heuristic extraction of architectural-decision candidates from messages.
//!
//! v1 is intentionally LLM-free: we segment message text into sentences, score
//! each one against a small bag of decision-marker phrases, and surface the
//! top-scoring sentences with their citation ref. Agents that want richer
//! analysis can post-process the output (e.g. send the citation refs to a
//! model with `aghist show`).
//!
//! The marker list and weights are intentionally conservative — we'd rather
//! miss a soft commitment than flood agents with low-signal noise. Tune via
//! `--threshold` at the CLI rather than rebalancing weights here.

use chrono::{DateTime, Utc};

use crate::model::{ContentBlock, Message, Role};

/// One ranked candidate decision sentence within a session.
#[derive(Debug, Clone)]
pub struct DecisionCandidate {
    /// 1-based turn number of the source message within its session.
    pub turn: u32,
    pub role: Role,
    pub timestamp: DateTime<Utc>,
    pub score: f32,
    /// Human-readable marker labels (lowercased, deduped) that fired on
    /// the sentence. Useful for explaining *why* the candidate ranked.
    pub markers: Vec<String>,
    /// Trimmed sentence, truncated to [`MAX_SNIPPET_CHARS`] for table display.
    pub snippet: String,
}

/// Default minimum score for a sentence to be returned. Calibrated so that
/// a single soft marker (e.g. just "should" or "because") is dropped, but
/// any explicit decision phrase ("we decided", "decided to", "instead of")
/// passes on its own.
pub const DEFAULT_THRESHOLD: f32 = 3.0;

const MAX_SNIPPET_CHARS: usize = 240;
const MAX_SENTENCE_BYTES: usize = 4 * 1024;

/// (lowercase needle, weight, display label).
///
/// Needles are matched as case-insensitive substrings. Order is irrelevant
/// — we sum every match. Where a needle includes a trailing space, that's
/// a deliberate word-boundary guard ("we will " avoids matching "we willingly").
const PATTERNS: &[(&str, f32, &str)] = &[
    // ─── explicit decision language (high signal) ──────────────────────
    ("we decided", 5.0, "we decided"),
    ("decided to", 5.0, "decided to"),
    ("decision:", 5.0, "decision:"),
    ("the decision is", 5.0, "the decision is"),
    ("agreed to", 5.0, "agreed to"),
    ("agreed that", 5.0, "agreed that"),
    ("chose to", 4.0, "chose to"),
    ("going with", 4.0, "going with"),
    ("settled on", 4.0, "settled on"),
    // ─── plans / commitments ──────────────────────────────────────────
    ("we will ", 3.0, "we will"),
    ("we won't", 3.0, "we won't"),
    ("we won\u{2019}t", 3.0, "we won't"),
    ("we'll ", 3.0, "we'll"),
    ("we\u{2019}ll ", 3.0, "we'll"),
    ("we shall", 3.0, "we shall"),
    ("we should ", 2.0, "we should"),
    // ─── comparative choice ──────────────────────────────────────────
    ("instead of", 3.0, "instead of"),
    ("rather than", 3.0, "rather than"),
    ("not ", 0.0, "not"), // placeholder — see SOFT below
    // ─── soft markers (only push borderline cases over the threshold) ──
    ("let's ", 2.0, "let's"),
    ("let us ", 2.0, "let us"),
    ("should ", 1.0, "should"),
    ("because ", 1.0, "because"),
];

// The "not " entry above is a placeholder so the table format stays uniform —
// strip it back out of the live patterns.
fn live_patterns() -> impl Iterator<Item = &'static (&'static str, f32, &'static str)> {
    PATTERNS.iter().filter(|(_, w, _)| *w > 0.0)
}

/// Extract candidates from one message, scoping the citation turn so the
/// caller can fan out to many messages without the per-message helper
/// needing the full session.
pub fn extract_from_message(
    msg: &Message,
    turn: u32,
    threshold: f32,
) -> Vec<DecisionCandidate> {
    let mut out = Vec::new();
    for block in &msg.content {
        let text = match block {
            ContentBlock::Text(t) | ContentBlock::Thinking(t) => t.as_str(),
            // Code, tool calls, and tool results rarely encode a decision in
            // running prose — and the noise-to-signal ratio is poor (e.g.
            // shell commands containing "because" by accident).
            _ => continue,
        };
        for sentence in split_sentences(text) {
            let scored = score_sentence(sentence);
            if scored.score >= threshold {
                out.push(DecisionCandidate {
                    turn,
                    role: msg.role,
                    timestamp: msg.timestamp,
                    score: scored.score,
                    markers: scored.markers,
                    snippet: clip(sentence, MAX_SNIPPET_CHARS),
                });
            }
        }
    }
    out
}

/// Extract candidates across an entire session in one shot. `messages` is
/// expected in load order — turn numbers are assigned by index.
pub fn extract_from_messages(
    messages: &[Message],
    threshold: f32,
) -> Vec<DecisionCandidate> {
    let mut out = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        let turn = u32::try_from(i + 1).unwrap_or(u32::MAX);
        out.extend(extract_from_message(m, turn, threshold));
    }
    out
}

struct Scored {
    score: f32,
    markers: Vec<String>,
}

fn score_sentence(sentence: &str) -> Scored {
    let lower = sentence.to_lowercase();
    let mut score = 0.0;
    let mut markers: Vec<String> = Vec::new();
    for (needle, weight, label) in live_patterns() {
        if lower.contains(needle) {
            score += weight;
            let label = (*label).to_string();
            if !markers.contains(&label) {
                markers.push(label);
            }
        }
    }
    Scored { score, markers }
}

/// Split a text blob into sentences on `.`/`!`/`?`/newline boundaries.
///
/// Cheap and naive: doesn't try to handle abbreviations or ellipses, and
/// preserves the terminator with its sentence so snippets read naturally.
/// Sentences longer than [`MAX_SENTENCE_BYTES`] are truncated to keep
/// pathological inputs (e.g. minified JSON dumped into a chat) from blowing
/// the snippet column.
fn split_sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let bytes = text.len();
    for (i, ch) in text.char_indices() {
        if matches!(ch, '.' | '!' | '?' | '\n') {
            let end = i + ch.len_utf8();
            push_segment(text, start, end, &mut out);
            start = end;
        }
    }
    if start < bytes {
        push_segment(text, start, bytes, &mut out);
    }
    out
}

fn push_segment<'a>(text: &'a str, start: usize, end: usize, out: &mut Vec<&'a str>) {
    let seg = &text[start..end];
    if seg.trim().is_empty() {
        return;
    }
    let bounded = if seg.len() > MAX_SENTENCE_BYTES {
        // Find the last char boundary at or below the limit so we don't
        // panic on a multi-byte boundary.
        let mut cut = MAX_SENTENCE_BYTES;
        while cut > 0 && !seg.is_char_boundary(cut) {
            cut -= 1;
        }
        &seg[..cut]
    } else {
        seg
    };
    out.push(bounded);
}

fn clip(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentBlock, Message, MessageId, Role};
    use chrono::TimeZone;

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
        assert!(candidates.is_empty(), "lone 'because' should not pass: {candidates:?}");
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
        // "we should" (2.0) + "should" (1.0) = 3.0 — above default
        let lo = extract_from_message(&m, 1, DEFAULT_THRESHOLD);
        assert_eq!(lo.len(), 1);
        // bump threshold past the score → drop
        let hi = extract_from_message(&m, 1, 5.0);
        assert!(hi.is_empty());
    }
}
