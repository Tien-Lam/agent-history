use crate::model::{CitationRef, Message};

use super::SNIPPET_MAX;
use crate::todos::{TodoCandidate, TodoKind};

use self::bd_ref::find_bd_refs;
use self::todo_keyword::contains_todo_keyword;

mod bd_ref;
mod todo_keyword;

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

fn snippet_of(line: &str) -> String {
    if line.chars().count() <= SNIPPET_MAX {
        return line.to_string();
    }
    let mut s: String = line.chars().take(SNIPPET_MAX - 1).collect();
    s.push('…');
    s
}
