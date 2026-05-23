use crate::model::{CitationRef, Message};

use super::SNIPPET_MAX;
use crate::todos::{TodoCandidate, TodoKind};

pub(super) fn scan_line(
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

fn want(kinds: &[TodoKind], k: TodoKind) -> bool {
    kinds.is_empty() || kinds.contains(&k)
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
