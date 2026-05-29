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

use self::scoring::score_sentence;
use self::text::{clip, split_sentences};

mod scoring;
mod text;

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
    /// Trimmed sentence, truncated to the table-display snippet limit.
    pub snippet: String,
}

/// Default minimum score for a sentence to be returned. Calibrated so that
/// a single soft marker (e.g. just "should" or "because") is dropped, but
/// any explicit decision phrase ("we decided", "decided to", "instead of")
/// passes on its own.
pub const DEFAULT_THRESHOLD: f32 = 3.0;

const MAX_SNIPPET_CHARS: usize = 240;

/// Extract candidates from one message, scoping the citation turn so the
/// caller can fan out to many messages without the per-message helper
/// needing the full session.
pub fn extract_from_message(msg: &Message, turn: u32, threshold: f32) -> Vec<DecisionCandidate> {
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
pub fn extract_from_messages(messages: &[Message], threshold: f32) -> Vec<DecisionCandidate> {
    let mut out = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        let turn = u32::try_from(i + 1).unwrap_or(u32::MAX);
        out.extend(extract_from_message(m, turn, threshold));
    }
    out
}

#[cfg(test)]
mod tests;
