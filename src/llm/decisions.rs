use serde::{Deserialize, Serialize};

use super::common::{post_request, LlmConfig, LlmError, LlmTransport};
use crate::model::{CitationRef, Provider, Role, SessionId};

mod prompt;
mod response;

pub use prompt::{build_request_body, user_message, SYSTEM_PROMPT};
pub use response::parse_response;

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
