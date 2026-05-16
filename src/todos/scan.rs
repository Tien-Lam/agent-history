use crate::model::{CitationRef, ContentBlock, Message, Provider, SessionId};

use super::{TodoCandidate, TodoKind};

/// Maximum snippet length emitted per candidate. Keeps the JSON output
/// scannable when long lines (e.g. minified tool args) match a keyword.
pub(super) const SNIPPET_MAX: usize = 240;

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
