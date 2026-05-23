use serde::{Deserialize, Serialize};

use super::common::{post_request, LlmConfig, LlmError, LlmTransport};
use crate::model::{CitationRef, Provider, Role, SessionId};

mod prompt;
mod response;
mod target;

pub use prompt::{build_todos_request_body, user_message_todos, SYSTEM_PROMPT_TODOS};
pub use response::parse_todos_response;
pub(super) use target::sanitize_target_session;

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
