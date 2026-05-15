use serde::{Deserialize, Serialize};

use super::common::{
    build_request_body_with_system, post_request, response_json_object, role_label, LlmConfig,
    LlmError, LlmTransport,
};
use crate::model::{CitationRef, Provider, Role, SessionId};

/// System prompt for the todos extractor. Same caching contract as the
/// other prompts: byte-stable across calls so the prompt cache hits on
/// call N>=2.
pub const SYSTEM_PROMPT_TODOS: &str = "\
You are a TODO/follow-up triager for a developer's AI-coding-agent history. \
Each input lists candidate TODOs surfaced by a regex heuristic — raw matched \
lines from one session's transcript. For each candidate that names a real \
piece of unfinished work (not a question, not a tool-name like `TodoWrite`, \
not a duplicate of an earlier TODO in the same input), return a structured \
record.

Schema, JSON only, no prose:
{\"todos\":[{\"description\":\"<one short imperative phrase>\",\"raised_at\":<integer turn>,\"target_session\":\"<optional provider/session-id or bd-ref like ahist-y3o.7.2>\",\"status_inferred\":\"open|done|unclear\"}]}

Rules:
- description: short imperative phrase naming the work to do (\"Revisit BM25 ranking\", \"Wire up SQLite migration\"). One phrase, present tense.
- raised_at: copy the turn integer from the candidate that anchored the TODO.
- target_session: include ONLY if the snippet explicitly names another session, file, or beads-style id where the followup should land (e.g. `ahist-y3o.7.2`, `claude-code/abc-123`). Omit the field if not stated.
- status_inferred: `open` if the language reads as still-pending; `done` if the snippet itself indicates the TODO has been addressed/closed; `unclear` otherwise.
- Skip candidates that are merely tool-name mentions (`TodoWrite`, `TodoCreate`), questions, restated commitments, or duplicates of an earlier TODO in the same input.
- If no candidates name real work, return {\"todos\":[]}.";

/// One heuristic todo candidate as input to the extractor. Decoupled from
/// `crate::todos::TodoCandidate` to keep this module test-isolated.
#[derive(Debug, Clone)]
pub struct TodoCandidate<'a> {
    pub turn: u32,
    pub role: Role,
    pub kind: &'a str,
    pub snippet: &'a str,
}

/// Per-session input to the todos extractor.
#[derive(Debug)]
pub struct TodoExtractionInput<'a> {
    pub provider: Provider,
    pub session_id: &'a SessionId,
    pub project: Option<&'a str>,
    pub candidates: Vec<TodoCandidate<'a>>,
}

/// Inferred status from the LLM. `Unclear` is the safe default when the
/// model can't tell whether a TODO is still pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TodoStatus {
    Open,
    Done,
    Unclear,
}

/// One structured todo from the LLM, anchored to a turn within a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredTodo {
    pub description: String,
    pub raised_at: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_session: Option<String>,
    pub status_inferred: TodoStatus,
}

/// Output: structured todos paired with their stable citation refs.
#[derive(Debug, Clone)]
pub struct ExtractedTodo {
    pub citation: CitationRef,
    pub todo: StructuredTodo,
    /// The heuristic snippet that anchored this todo (for traceability).
    pub source_snippet: Option<String>,
    /// The heuristic kind slug (e.g. `todo`, `follow-up`, `bd-ref`) that
    /// matched on the anchoring line. Empty when no candidate matched the
    /// turn (e.g. the model invented a turn).
    pub source_kind: Option<String>,
}

/// Build the user-message body for one session's todo candidates.
#[must_use]
pub fn user_message_todos(input: &TodoExtractionInput<'_>) -> String {
    let mut s = String::new();
    if let Some(p) = input.project {
        s.push_str("Project: ");
        s.push_str(p);
        s.push('\n');
    }
    s.push_str("Session: ");
    s.push_str(input.provider.slug());
    s.push('/');
    s.push_str(&input.session_id.0);
    s.push_str("\n\nCandidate TODOs:\n");
    for c in &input.candidates {
        s.push_str("- turn ");
        s.push_str(&c.turn.to_string());
        s.push_str(" (");
        s.push_str(role_label(c.role));
        s.push_str(", kind=");
        s.push_str(c.kind);
        s.push_str("): ");
        // Collapse newlines so each candidate stays on a single row — the
        // model treats `\n` as a row delimiter.
        for ch in c.snippet.chars() {
            if ch == '\n' || ch == '\r' {
                s.push(' ');
            } else {
                s.push(ch);
            }
        }
        s.push('\n');
    }
    s
}

/// Build the request body for a todos-extraction API call.
pub fn build_todos_request_body(config: &LlmConfig, user: &str) -> Result<String, LlmError> {
    build_request_body_with_system(config, SYSTEM_PROMPT_TODOS, user)
}

#[derive(Deserialize)]
struct TodosPayload {
    todos: Vec<StructuredTodo>,
}

/// Parse the Messages API response body into structured todos. Same
/// JSON-extraction tolerance as `parse_response`.
pub fn parse_todos_response(body: &str) -> Result<Vec<StructuredTodo>, LlmError> {
    let json_slice = response_json_object(body)?;
    let parsed: TodosPayload = serde_json::from_str(&json_slice).map_err(|e| {
        LlmError::Parse(format!(
            "todos payload: {e} (slice starts: {})",
            json_slice.chars().take(80).collect::<String>()
        ))
    })?;
    Ok(parsed.todos)
}

/// Validate a `target_session` string. Accepts either a citation-style
/// `<provider-slug>/<session-id>` ref or a beads-style `<prefix>-<suffix>`
/// id (prefix = 2+ lowercase letters, suffix has at least one digit). Drops
/// anything else so the LLM can't smuggle in arbitrary strings.
pub(super) fn sanitize_target_session(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Drop trailing `#turn` if present — the schema says session-level only.
    let head = trimmed.split_once('#').map_or(trimmed, |(h, _)| h);
    if head.is_empty() {
        return None;
    }
    if let Some((slug, rest)) = head.split_once('/') {
        if !rest.is_empty() && Provider::all().iter().any(|p| p.slug() == slug) {
            return Some(head.to_string());
        }
    }
    // Beads-style id: at least two lowercase letters, then `-`, then a
    // suffix containing at least one digit. Mirrors `crate::todos::find_bd_refs`.
    if let Some((prefix, suffix)) = head.split_once('-') {
        let prefix_ok = prefix.len() >= 2 && prefix.bytes().all(|b| b.is_ascii_lowercase());
        let suffix_ok = !suffix.is_empty()
            && suffix
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.')
            && suffix.bytes().any(|b| b.is_ascii_digit());
        if prefix_ok && suffix_ok {
            return Some(head.to_string());
        }
    }
    None
}

/// Run extraction for one session's todo candidates. Mirrors
/// `extract_for_session` for decisions: builds the request, posts it,
/// parses the response, attaches stable citation refs, and validates any
/// inferred `target_session` so hallucinated refs are dropped.
pub fn extract_for_session_todos<T: LlmTransport + ?Sized>(
    transport: &T,
    config: &LlmConfig,
    input: &TodoExtractionInput<'_>,
) -> Result<Vec<ExtractedTodo>, LlmError> {
    if input.candidates.is_empty() {
        return Ok(Vec::new());
    }
    let user = user_message_todos(input);
    let body = build_todos_request_body(config, &user)?;
    let resp_body = post_request(transport, config, &body)?;
    let todos = parse_todos_response(&resp_body)?;
    let mut out = Vec::with_capacity(todos.len());
    for mut t in todos {
        if t.description.trim().is_empty() {
            continue;
        }
        let Some(citation) =
            CitationRef::new(input.provider, input.session_id.clone(), t.raised_at)
        else {
            continue;
        };
        let anchor = input.candidates.iter().find(|c| c.turn == t.raised_at);
        let source_snippet = anchor.map(|c| c.snippet.to_string());
        let source_kind = anchor.map(|c| c.kind.to_string());
        t.target_session = t.target_session.and_then(|s| sanitize_target_session(&s));
        out.push(ExtractedTodo {
            citation,
            todo: t,
            source_snippet,
            source_kind,
        });
    }
    Ok(out)
}
