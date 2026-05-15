use serde::{Deserialize, Serialize};

use super::common::{
    build_request_body_with_system, post_request, response_json_object, role_label, LlmConfig,
    LlmError, LlmTransport,
};
use crate::model::{CitationRef, Provider, Role, SessionId};

/// One structured decision from the LLM, anchored to a turn within a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredDecision {
    pub summary: String,
    pub rationale: String,
    #[serde(default)]
    pub alternatives: Vec<String>,
    pub turn: u32,
}

/// One ranked candidate sentence from the heuristic, in the shape the
/// extractor wants. We keep this decoupled from `decisions::DecisionCandidate`
/// to avoid a circular crate-internal dependency in tests.
#[derive(Debug, Clone)]
pub struct Candidate<'a> {
    pub turn: u32,
    pub role: Role,
    pub snippet: &'a str,
}

/// Per-session input to the extractor.
#[derive(Debug)]
pub struct ExtractionInput<'a> {
    pub provider: Provider,
    pub session_id: &'a SessionId,
    pub project: Option<&'a str>,
    pub candidates: Vec<Candidate<'a>>,
}

/// Output: structured decisions paired with their stable citation refs.
#[derive(Debug, Clone)]
pub struct ExtractedDecision {
    pub citation: CitationRef,
    pub decision: StructuredDecision,
    /// The heuristic snippet that anchored this decision (for traceability).
    pub source_snippet: Option<String>,
}

/// System prompt — kept short and instruction-dense. Cacheable via
/// `cache_control: ephemeral`, so we want every call to send the *exact*
/// same bytes here.
pub const SYSTEM_PROMPT: &str = "\
You are an architectural-decision extractor. Each input lists candidate \
sentences from a developer's AI-coding-agent conversation. Identify which \
candidates represent real decisions actually made (not hypotheticals, not \
restatements of earlier decisions, not questions). For each real decision, \
return a structured record.

Schema, JSON only, no prose:
{\"decisions\":[{\"summary\":\"<one imperative sentence>\",\"rationale\":\"<reasoning if stated, else empty>\",\"alternatives\":[\"<rejected option>\"],\"turn\":<integer>}]}

Rules:
- summary: imperative, present tense (\"Use BM25 over cosine\", \"Drop the cache layer\"). One sentence.
- rationale: paraphrase the *why* if the source text gives one; empty string if not.
- alternatives: only options explicitly mentioned and rejected. Empty array if none.
- turn: copy the turn integer from the candidate that anchors the decision.
- Skip candidates that are merely speculation, questions, or duplicates of an earlier decision in the same input.
- If the input contains no real decisions, return {\"decisions\":[]}.";

/// Build the user-message body for one session's candidates.
#[must_use]
pub fn user_message(input: &ExtractionInput<'_>) -> String {
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
    s.push_str("\n\nCandidate sentences:\n");
    for c in &input.candidates {
        s.push_str("- turn ");
        s.push_str(&c.turn.to_string());
        s.push_str(" (");
        s.push_str(role_label(c.role));
        s.push_str("): ");
        s.push_str(c.snippet);
        s.push('\n');
    }
    s
}

/// Build the JSON request body for one extraction call. Public so callers
/// (and tests) can inspect what gets sent on the wire.
pub fn build_request_body(config: &LlmConfig, user: &str) -> Result<String, LlmError> {
    build_request_body_with_system(config, SYSTEM_PROMPT, user)
}

#[derive(Deserialize)]
struct DecisionsPayload {
    decisions: Vec<StructuredDecision>,
}

/// Parse the Messages API response body into structured decisions.
///
/// The model is asked for JSON only, but real-world models occasionally
/// wrap output in ` ```json ... ``` ` fences or add a trailing prose
/// sentence. We extract the first JSON object from the assistant text and
/// parse that, so small formatting drift doesn't break the pipeline.
pub fn parse_response(body: &str) -> Result<Vec<StructuredDecision>, LlmError> {
    let json_slice = response_json_object(body)?;
    let parsed: DecisionsPayload = serde_json::from_str(&json_slice).map_err(|e| {
        LlmError::Parse(format!(
            "decisions payload: {e} (slice starts: {})",
            json_slice.chars().take(80).collect::<String>()
        ))
    })?;
    Ok(parsed.decisions)
}

/// Run extraction for one session. Pure routing on top of [`LlmTransport`]:
/// builds the request body, posts it, parses the response, attaches stable
/// citation refs.
pub fn extract_for_session<T: LlmTransport + ?Sized>(
    transport: &T,
    config: &LlmConfig,
    input: &ExtractionInput<'_>,
) -> Result<Vec<ExtractedDecision>, LlmError> {
    if input.candidates.is_empty() {
        return Ok(Vec::new());
    }
    let user = user_message(input);
    let body = build_request_body(config, &user)?;
    let resp_body = post_request(transport, config, &body)?;
    let decisions = parse_response(&resp_body)?;
    let mut out = Vec::with_capacity(decisions.len());
    for d in decisions {
        let Some(citation) = CitationRef::new(input.provider, input.session_id.clone(), d.turn)
        else {
            continue;
        };
        let source_snippet = input
            .candidates
            .iter()
            .find(|c| c.turn == d.turn)
            .map(|c| c.snippet.to_string());
        out.push(ExtractedDecision {
            citation,
            decision: d,
            source_snippet,
        });
    }
    Ok(out)
}
