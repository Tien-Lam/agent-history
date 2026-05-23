use super::super::common::{build_request_body_with_system, role_label, LlmConfig, LlmError};
use super::ExtractionInput;

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
