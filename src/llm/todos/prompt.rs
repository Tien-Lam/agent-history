use super::super::common::{build_request_body_with_system, role_label, LlmConfig, LlmError};
use super::TodoExtractionInput;

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
{\"todos\":[{\"description\":\"<one short imperative phrase>\",\"raised_at\":<integer turn>,\"target_session\":\"<optional provider/session-id, source:provider/session-id, or bd-ref like ahist-y3o.7.2>\",\"status_inferred\":\"open|done|unclear\"}]}

Rules:
- description: short imperative phrase naming the work to do (\"Revisit BM25 ranking\", \"Wire up SQLite migration\"). One phrase, present tense.
- raised_at: copy the turn integer from the candidate that anchored the TODO.
- target_session: include ONLY if the snippet explicitly names another session, file, or beads-style id where the followup should land (e.g. `ahist-y3o.7.2`, `claude-code/abc-123`, `laptop:claude-code/abc-123`). Omit the field if not stated.
- status_inferred: `open` if the language reads as still-pending; `done` if the snippet itself indicates the TODO has been addressed/closed; `unclear` otherwise.
- Skip candidates that are merely tool-name mentions (`TodoWrite`, `TodoCreate`), questions, restated commitments, or duplicates of an earlier TODO in the same input.
- If no candidates name real work, return {\"todos\":[]}.";

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
