//! Heuristic extraction of unresolved TODOs / follow-ups / open beads
//! from session transcripts.
//!
//! Spike for `aghist todos` (ahist-y3o.7.2). The bead epic
//! "Decision/TODO/thread extraction (research)" calls for a heuristic-first
//! v1: regex-style scans, deterministic output, no LLM. Agents can chain
//! follow-up extraction (e.g. drop refs whose `bd show` reports closed) on
//! top of the JSON shape produced here.
//!
//! Heuristics scanned per line:
//!   * `TODO` keyword (uppercase, word-bounded — avoids `todo` in prose
//!     and the literal tool name `TodoWrite` which appears constantly in
//!     Claude Code transcripts).
//!   * `follow-up` / `follow up` (case-insensitive).
//!   * `come back to` (case-insensitive).
//!   * `we should` (case-insensitive).
//!   * Beads-style refs `<prefix>-<suffix>` where prefix is 2+ lowercase
//!     letters and suffix has at least one digit (e.g. `ahist-y3o.7.2`,
//!     `gt-abc1`). Word-bounded so `follow-up` doesn't match.
//!
//! Each match becomes one [`TodoCandidate`] keyed by a [`CitationRef`] so
//! the caller can quote, deep-link, or `aghist show` the originating turn.

use serde::Serialize;

use crate::model::{CitationRef, ContentBlock, Message, Provider, Role, SessionId};

/// What kind of heuristic surfaced a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoKind {
    Todo,
    FollowUp,
    ComeBackTo,
    WeShould,
    BdRef,
}

impl TodoKind {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::FollowUp => "follow-up",
            Self::ComeBackTo => "come-back-to",
            Self::WeShould => "we-should",
            Self::BdRef => "bd-ref",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "todo" => Some(Self::Todo),
            "follow-up" | "followup" => Some(Self::FollowUp),
            "come-back-to" | "comebackto" => Some(Self::ComeBackTo),
            "we-should" | "weshould" => Some(Self::WeShould),
            "bd-ref" | "bdref" | "bd" => Some(Self::BdRef),
            _ => None,
        }
    }

    pub const ALL: [Self; 5] = [
        Self::Todo,
        Self::FollowUp,
        Self::ComeBackTo,
        Self::WeShould,
        Self::BdRef,
    ];
}

/// One surfaced candidate. `snippet` is the raw matched line, trimmed.
#[derive(Debug, Clone, Serialize)]
pub struct TodoCandidate {
    pub citation: CitationRef,
    pub kind: TodoKind,
    pub snippet: String,
    pub role: Role,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Set only for [`TodoKind::BdRef`] — the captured ID (e.g. `ahist-y3o.7.2`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bd_id: Option<String>,
}

/// Maximum snippet length emitted per candidate. Keeps the JSON output
/// scannable when long lines (e.g. minified tool args) match a keyword.
const SNIPPET_MAX: usize = 240;

/// Scan a session's messages and return every candidate matching `kinds`.
///
/// `kinds` empty means "all kinds". Order is deterministic: messages in
/// load order, lines within a message in document order, kinds in the
/// order they're scanned (TODO, follow-up, come-back-to, we-should, bd-ref).
pub fn extract_from_messages(
    provider: Provider,
    session_id: &SessionId,
    messages: &[Message],
    kinds: &[TodoKind],
) -> Vec<TodoCandidate> {
    let mut out = Vec::new();
    for (idx, msg) in messages.iter().enumerate() {
        let turn = u32::try_from(idx + 1).unwrap_or(u32::MAX);
        let Some(citation) = CitationRef::new(provider, session_id.clone(), turn) else {
            continue;
        };
        let text = collect_text(msg);
        for raw_line in text.lines() {
            scan_line(&citation, msg, raw_line, kinds, &mut out);
        }
    }
    out
}

fn want(kinds: &[TodoKind], k: TodoKind) -> bool {
    kinds.is_empty() || kinds.contains(&k)
}

fn scan_line(
    citation: &CitationRef,
    msg: &Message,
    line: &str,
    kinds: &[TodoKind],
    out: &mut Vec<TodoCandidate>,
) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    // The literal tool name `TodoWrite` shows up in nearly every Claude Code
    // transcript and would dominate output otherwise.
    let lower = trimmed.to_ascii_lowercase();
    let snippet = snippet_of(trimmed);

    if want(kinds, TodoKind::Todo) && contains_todo_keyword(trimmed) {
        push(out, citation, msg, TodoKind::Todo, snippet.clone(), None);
    }
    if want(kinds, TodoKind::FollowUp)
        && (lower.contains("follow-up") || lower.contains("follow up"))
    {
        push(
            out,
            citation,
            msg,
            TodoKind::FollowUp,
            snippet.clone(),
            None,
        );
    }
    if want(kinds, TodoKind::ComeBackTo) && lower.contains("come back to") {
        push(
            out,
            citation,
            msg,
            TodoKind::ComeBackTo,
            snippet.clone(),
            None,
        );
    }
    if want(kinds, TodoKind::WeShould) && lower.contains("we should") {
        push(
            out,
            citation,
            msg,
            TodoKind::WeShould,
            snippet.clone(),
            None,
        );
    }
    if want(kinds, TodoKind::BdRef) {
        for id in find_bd_refs(trimmed) {
            push(
                out,
                citation,
                msg,
                TodoKind::BdRef,
                snippet.clone(),
                Some(id),
            );
        }
    }
}

fn push(
    out: &mut Vec<TodoCandidate>,
    citation: &CitationRef,
    msg: &Message,
    kind: TodoKind,
    snippet: String,
    bd_id: Option<String>,
) {
    out.push(TodoCandidate {
        citation: citation.clone(),
        kind,
        snippet,
        role: msg.role,
        timestamp: msg.timestamp,
        bd_id,
    });
}

/// Word-bounded uppercase `TODO` match. Rejects `Todo`, `todo`, and any
/// occurrence inside `TodoWrite` (Claude Code tool name) or `TodoCreate`.
fn contains_todo_keyword(line: &str) -> bool {
    let bytes = line.as_bytes();
    let needle = b"TODO";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after = i + needle.len();
            let after_ok = after == bytes.len() || !is_ident_char(bytes[after]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Find beads-style refs in a line.
///
/// Pattern: `[a-z]{2,}-[a-z0-9.]+` with at least one digit in the suffix.
/// Word-bounded: must not be preceded or followed by another word/dash
/// character. The digit requirement filters out `follow-up`, `come-back-to`,
/// and similar prose hyphenates while still catching `ahist-y3o.7.2`,
/// `gt-abc1`, `pr-1234`, etc.
fn find_bd_refs(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let prefix_start = i;
        let before_ok = i == 0 || !is_word_char(bytes[i - 1]);
        if !before_ok || !bytes[i].is_ascii_lowercase() {
            i += 1;
            continue;
        }
        while i < bytes.len() && bytes[i].is_ascii_lowercase() {
            i += 1;
        }
        let prefix_len = i - prefix_start;
        if prefix_len < 2 || i >= bytes.len() || bytes[i] != b'-' {
            continue;
        }
        let suffix_start = i + 1;
        let mut j = suffix_start;
        while j < bytes.len() {
            let c = bytes[j];
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' {
                j += 1;
            } else {
                break;
            }
        }
        // Trailing dot is punctuation, not part of the id.
        while j > suffix_start && bytes[j - 1] == b'.' {
            j -= 1;
        }
        let after_ok = j == bytes.len() || !is_word_char(bytes[j]);
        let suffix = &line[suffix_start..j];
        let has_digit = suffix.bytes().any(|c| c.is_ascii_digit());
        if after_ok && !suffix.is_empty() && has_digit {
            out.push(line[prefix_start..j].to_string());
        }
        i = j.max(i + 1);
    }
    out
}

fn snippet_of(line: &str) -> String {
    if line.chars().count() <= SNIPPET_MAX {
        return line.to_string();
    }
    let mut s: String = line.chars().take(SNIPPET_MAX - 1).collect();
    s.push('…');
    s
}

fn collect_text(message: &Message) -> String {
    let parts: Vec<&str> = message
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text(t) | ContentBlock::Thinking(t) | ContentBlock::Error(t) => {
                t.as_str()
            }
            ContentBlock::CodeBlock { code, .. } => code.as_str(),
            ContentBlock::ToolUse(tc) => tc.arguments.as_str(),
            ContentBlock::ToolResult(tr) => tr.output.as_str(),
        })
        .collect();
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentBlock, Message, MessageId, Provider, Role, SessionId};
    use chrono::TimeZone;

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
        // Two hits: bd-ref + follow-up
        assert!(hits.iter().any(|h| h.kind == TodoKind::BdRef));
        let bd = hits.iter().find(|h| h.kind == TodoKind::BdRef).unwrap();
        assert_eq!(bd.bd_id.as_deref(), Some("ahist-y3o.7.2"));
    }

    #[test]
    fn bd_ref_ignores_prose_hyphenates() {
        // "follow-up" must not match as a bd ref — no digits in suffix.
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
}
