//! Optional LLM-backed structured decision extraction.
//!
//! The default `aghist decisions` flow is a deterministic regex/marker
//! heuristic (see [`crate::decisions`]). This module adds an opt-in routing
//! layer for `aghist decisions --llm`: heuristic candidates become input to
//! a Claude-shaped Messages API call that returns structured records of the
//! form `{summary, rationale, alternatives, turn}`.
//!
//! Why opt-in: the heuristic is free, deterministic, and good enough for
//! quick triage. The LLM path costs tokens and adds non-determinism, but
//! produces decisions that read like an architectural log instead of raw
//! sentence slices.
//!
//! ## Configuration
//!
//! All config is environment-driven. The CLI does not accept secrets via
//! flags (so they never land in shell history or process listings):
//!
//! - `ANTHROPIC_API_KEY` (or `AGHIST_LLM_API_KEY`) — required.
//! - `AGHIST_LLM_ENDPOINT` — full Messages API URL. Default
//!   `https://api.anthropic.com/v1/messages`. Override to point at a
//!   local/proxied Anthropic-compatible endpoint.
//! - `AGHIST_LLM_MODEL` — model id. Default
//!   `claude-haiku-4-5-20251001` (cheap, fast, good enough for this task).
//! - `AGHIST_LLM_ANTHROPIC_VERSION` — `anthropic-version` header. Default
//!   `2023-06-01`.
//!
//! ## Prompt caching
//!
//! The system prompt is the largest static block per call. It's marked with
//! `cache_control: ephemeral` so subsequent calls in the same invocation
//! (one per session) hit Anthropic's prompt cache and pay near-zero for
//! the system tokens. This is the cost lever called out in the bead.
//!
//! ## Testability
//!
//! Network IO goes through the [`LlmTransport`] trait. Tests use
//! `MockTransport` to drive the extractor end-to-end without sockets.

use std::env;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::{CitationRef, Provider, Role, SessionId};

/// Errors surfaced from the LLM extraction path. Mapped to the
/// `llm-error` envelope kind at the CLI boundary.
#[derive(Debug, Error)]
pub enum LlmError {
    #[error(
        "missing API key: set ANTHROPIC_API_KEY (or AGHIST_LLM_API_KEY) before running --llm"
    )]
    MissingApiKey,
    #[error("HTTP request to {url} failed: {source}")]
    Http {
        url: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("API at {url} returned status {status}: {body}")]
    ApiStatus {
        url: String,
        status: u16,
        body: String,
    },
    #[error("could not parse LLM response: {0}")]
    Parse(String),
    #[error("model returned no parsable JSON in its reply: {0}")]
    NoJson(String),
}

/// Runtime configuration for the LLM extractor. Constructed via
/// [`LlmConfig::from_env`] in normal use; tests build it directly.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    pub anthropic_version: String,
    pub timeout: Duration,
}

impl LlmConfig {
    /// Cheap default — gives sensible quality on this task without paying for Sonnet.
    pub const DEFAULT_MODEL: &'static str = "claude-haiku-4-5-20251001";
    pub const DEFAULT_ENDPOINT: &'static str = "https://api.anthropic.com/v1/messages";
    pub const DEFAULT_VERSION: &'static str = "2023-06-01";
    pub const DEFAULT_MAX_TOKENS: u32 = 1024;
    pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

    /// Build config from process environment. `AGHIST_LLM_API_KEY` wins over
    /// `ANTHROPIC_API_KEY` so users can scope a separate key per tool.
    pub fn from_env() -> Result<Self, LlmError> {
        let api_key = env::var("AGHIST_LLM_API_KEY")
            .ok()
            .or_else(|| env::var("ANTHROPIC_API_KEY").ok())
            .filter(|s| !s.is_empty())
            .ok_or(LlmError::MissingApiKey)?;
        let endpoint = env::var("AGHIST_LLM_ENDPOINT")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Self::DEFAULT_ENDPOINT.to_string());
        let model = env::var("AGHIST_LLM_MODEL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Self::DEFAULT_MODEL.to_string());
        let anthropic_version = env::var("AGHIST_LLM_ANTHROPIC_VERSION")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Self::DEFAULT_VERSION.to_string());
        Ok(Self {
            endpoint,
            api_key,
            model,
            max_tokens: Self::DEFAULT_MAX_TOKENS,
            anthropic_version,
            timeout: Duration::from_secs(Self::DEFAULT_TIMEOUT_SECS),
        })
    }

    /// Override the model (e.g. from the CLI flag).
    #[must_use]
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }
}

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

/// HTTP transport boundary. Production uses [`UreqTransport`]; tests inject
/// a mock so the extractor can be exercised without a network.
pub trait LlmTransport: Send + Sync {
    /// POST `body` (JSON) to `url` with the supplied headers, returning
    /// `(status, body)`. Implementations MUST NOT raise on non-2xx — let
    /// the caller decide based on status.
    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<(u16, String), LlmError>;
}

/// `ureq`-backed transport. Synchronous to fit the rest of aghist's IO model.
pub struct UreqTransport {
    timeout: Duration,
}

impl UreqTransport {
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new(Duration::from_secs(LlmConfig::DEFAULT_TIMEOUT_SECS))
    }
}

impl LlmTransport for UreqTransport {
    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<(u16, String), LlmError> {
        let agent = ureq::AgentBuilder::new()
            .timeout(self.timeout)
            .build();
        let mut req = agent.post(url).set("content-type", "application/json");
        for (k, v) in headers {
            req = req.set(k, v);
        }
        match req.send_string(body) {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.into_string().map_err(|e| LlmError::Http {
                    url: url.to_string(),
                    source: Box::new(e),
                })?;
                Ok((status, text))
            }
            Err(ureq::Error::Status(status, resp)) => {
                let text = resp.into_string().unwrap_or_default();
                Ok((status, text))
            }
            Err(e) => Err(LlmError::Http {
                url: url.to_string(),
                source: Box::new(e),
            }),
        }
    }
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

fn role_label(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Serialize)]
struct SystemBlock<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'a str,
    cache_control: CacheControl,
}

#[derive(Serialize)]
struct UserMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    max_tokens: u32,
    system: [SystemBlock<'a>; 1],
    messages: [UserMessage<'a>; 1],
}

/// Build the JSON request body for one extraction call. Public so callers
/// (and tests) can inspect what gets sent on the wire.
pub fn build_request_body(config: &LlmConfig, user: &str) -> Result<String, LlmError> {
    let req = Request {
        model: &config.model,
        max_tokens: config.max_tokens,
        system: [SystemBlock {
            kind: "text",
            text: SYSTEM_PROMPT,
            cache_control: CacheControl { kind: "ephemeral" },
        }],
        messages: [UserMessage {
            role: "user",
            content: user,
        }],
    };
    serde_json::to_string(&req).map_err(|e| LlmError::Parse(e.to_string()))
}

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(default)]
    content: Vec<ApiContentBlock>,
}

#[derive(Deserialize)]
struct ApiContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
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
    let resp: ApiResponse = serde_json::from_str(body)
        .map_err(|e| LlmError::Parse(format!("response envelope: {e}")))?;
    let text = resp
        .content
        .into_iter()
        .find(|b| b.kind == "text")
        .map(|b| b.text)
        .unwrap_or_default();
    if text.trim().is_empty() {
        return Err(LlmError::NoJson("empty assistant text".into()));
    }
    let json_slice = extract_json_object(&text)
        .ok_or_else(|| LlmError::NoJson(text.chars().take(200).collect()))?;
    let parsed: DecisionsPayload = serde_json::from_str(json_slice).map_err(|e| {
        LlmError::Parse(format!(
            "decisions payload: {e} (slice starts: {})",
            json_slice.chars().take(80).collect::<String>()
        ))
    })?;
    Ok(parsed.decisions)
}

/// Find the first balanced `{...}` object in `text`. Returns `None` if no
/// balanced object is present. Skips over braces that appear inside double-
/// quoted strings (with `\\` escape handling) so JSON-with-prose still parses.
fn extract_json_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
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
    let headers: [(&str, &str); 3] = [
        ("x-api-key", config.api_key.as_str()),
        ("anthropic-version", config.anthropic_version.as_str()),
        ("content-type", "application/json"),
    ];
    let (status, resp_body) = transport.post_json(&config.endpoint, &headers, &body)?;
    if !(200..300).contains(&status) {
        return Err(LlmError::ApiStatus {
            url: config.endpoint.clone(),
            status,
            body: resp_body.chars().take(500).collect(),
        });
    }
    let decisions = parse_response(&resp_body)?;
    let mut out = Vec::with_capacity(decisions.len());
    for d in decisions {
        let Some(citation) =
            CitationRef::new(input.provider, input.session_id.clone(), d.turn)
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

#[cfg(test)]
pub mod test_support {
    use super::{LlmError, LlmTransport};
    use std::sync::Mutex;

    /// Test transport. Returns canned responses in order; records each
    /// outgoing request for assertion.
    pub struct MockTransport {
        pub responses: Mutex<Vec<(u16, String)>>,
        pub requests: Mutex<Vec<(String, String)>>,
    }

    impl MockTransport {
        #[must_use]
        pub fn new(responses: Vec<(u16, String)>) -> Self {
            Self {
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl LlmTransport for MockTransport {
        fn post_json(
            &self,
            url: &str,
            _headers: &[(&str, &str)],
            body: &str,
        ) -> Result<(u16, String), LlmError> {
            self.requests
                .lock()
                .unwrap()
                .push((url.to_string(), body.to_string()));
            let mut r = self.responses.lock().unwrap();
            if r.is_empty() {
                return Err(LlmError::Parse("mock: no responses left".into()));
            }
            Ok(r.remove(0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::MockTransport;
    use super::*;

    fn cfg() -> LlmConfig {
        LlmConfig {
            endpoint: "https://example.test/v1/messages".into(),
            api_key: "sk-test".into(),
            model: "claude-haiku-test".into(),
            max_tokens: 256,
            anthropic_version: "2023-06-01".into(),
            timeout: Duration::from_secs(5),
        }
    }

    fn input<'a>(
        sid: &'a SessionId,
        candidates: Vec<Candidate<'a>>,
    ) -> ExtractionInput<'a> {
        ExtractionInput {
            provider: Provider::ClaudeCode,
            session_id: sid,
            project: Some("aghist"),
            candidates,
        }
    }

    fn assistant_response(decisions_json: &str) -> String {
        format!(
            r#"{{"id":"msg_x","type":"message","role":"assistant","content":[{{"type":"text","text":{}}}],"model":"claude-haiku-test","stop_reason":"end_turn"}}"#,
            serde_json::to_string(decisions_json).unwrap()
        )
    }

    #[test]
    fn user_message_includes_project_provider_session_and_candidates() {
        let sid = SessionId("ses_abc".into());
        let inp = input(
            &sid,
            vec![Candidate {
                turn: 7,
                role: Role::Assistant,
                snippet: "We decided to use BM25.",
            }],
        );
        let msg = user_message(&inp);
        assert!(msg.contains("Project: aghist"));
        assert!(msg.contains("Session: claude-code/ses_abc"));
        assert!(msg.contains("- turn 7 (assistant): We decided to use BM25."));
    }

    #[test]
    fn build_request_body_includes_cache_control_on_system() {
        let body = build_request_body(&cfg(), "hi").unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["model"], "claude-haiku-test");
        assert_eq!(v["system"][0]["type"], "text");
        assert_eq!(v["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(v["messages"][0]["role"], "user");
        assert_eq!(v["messages"][0]["content"], "hi");
    }

    #[test]
    fn parse_response_handles_plain_json() {
        let inner = r#"{"decisions":[{"summary":"Use BM25","rationale":"better than cosine","alternatives":["cosine"],"turn":7}]}"#;
        let body = assistant_response(inner);
        let out = parse_response(&body).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].summary, "Use BM25");
        assert_eq!(out[0].turn, 7);
        assert_eq!(out[0].alternatives, vec!["cosine".to_string()]);
    }

    #[test]
    fn parse_response_extracts_json_from_code_fence() {
        let inner = "```json\n{\"decisions\":[{\"summary\":\"Drop cache\",\"rationale\":\"\",\"alternatives\":[],\"turn\":1}]}\n```";
        let body = assistant_response(inner);
        let out = parse_response(&body).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].summary, "Drop cache");
        assert!(out[0].alternatives.is_empty());
    }

    #[test]
    fn parse_response_handles_trailing_prose() {
        let inner = "{\"decisions\":[]}\nNo decisions found.";
        let body = assistant_response(inner);
        let out = parse_response(&body).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn parse_response_errors_on_no_json() {
        let body = assistant_response("the model refused");
        let err = parse_response(&body).unwrap_err();
        assert!(matches!(err, LlmError::NoJson(_)));
    }

    #[test]
    fn parse_response_errors_on_missing_text_block() {
        let body =
            r#"{"id":"msg_x","content":[{"type":"tool_use","name":"x","input":{}}]}"#;
        let err = parse_response(body).unwrap_err();
        assert!(matches!(err, LlmError::NoJson(_)));
    }

    #[test]
    fn extract_json_object_skips_braces_inside_strings() {
        let text = r#"prelude {"a":"has a } in it","b":1} trailing"#;
        let slice = extract_json_object(text).unwrap();
        assert_eq!(slice, r#"{"a":"has a } in it","b":1}"#);
    }

    #[test]
    fn extract_for_session_skips_when_no_candidates() {
        let mock = MockTransport::new(vec![]);
        let sid = SessionId("ses".into());
        let inp = input(&sid, vec![]);
        let out = extract_for_session(&mock, &cfg(), &inp).unwrap();
        assert!(out.is_empty());
        assert!(mock.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn extract_for_session_attaches_citation_refs() {
        let inner = r#"{"decisions":[{"summary":"Use BM25","rationale":"r","alternatives":[],"turn":7}]}"#;
        let mock = MockTransport::new(vec![(200, assistant_response(inner))]);
        let sid = SessionId("ses_abc".into());
        let cands = vec![Candidate {
            turn: 7,
            role: Role::Assistant,
            snippet: "We decided to use BM25.",
        }];
        let inp = input(&sid, cands);
        let out = extract_for_session(&mock, &cfg(), &inp).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].citation.to_string(), "claude-code/ses_abc#7");
        assert_eq!(out[0].decision.summary, "Use BM25");
        assert_eq!(
            out[0].source_snippet.as_deref(),
            Some("We decided to use BM25.")
        );
    }

    #[test]
    fn extract_for_session_drops_decisions_with_invalid_turn() {
        let inner = r#"{"decisions":[{"summary":"x","rationale":"","alternatives":[],"turn":0}]}"#;
        let mock = MockTransport::new(vec![(200, assistant_response(inner))]);
        let sid = SessionId("ses".into());
        let cands = vec![Candidate {
            turn: 1,
            role: Role::Assistant,
            snippet: "anything",
        }];
        let inp = input(&sid, cands);
        let out = extract_for_session(&mock, &cfg(), &inp).unwrap();
        assert!(out.is_empty(), "turn=0 must be rejected by CitationRef");
    }

    #[test]
    fn extract_for_session_propagates_api_errors() {
        let mock =
            MockTransport::new(vec![(429, r#"{"error":"rate limited"}"#.to_string())]);
        let sid = SessionId("ses".into());
        let cands = vec![Candidate {
            turn: 1,
            role: Role::Assistant,
            snippet: "hi",
        }];
        let inp = input(&sid, cands);
        let err = extract_for_session(&mock, &cfg(), &inp).unwrap_err();
        match err {
            LlmError::ApiStatus { status, .. } => assert_eq!(status, 429),
            other => panic!("expected ApiStatus, got {other:?}"),
        }
    }

    #[test]
    fn extract_for_session_sets_correct_headers_and_caches_system() {
        let inner = r#"{"decisions":[]}"#;
        let mock = MockTransport::new(vec![(200, assistant_response(inner))]);
        let sid = SessionId("ses".into());
        let inp = input(
            &sid,
            vec![Candidate {
                turn: 1,
                role: Role::User,
                snippet: "we decided to ship",
            }],
        );
        let _ = extract_for_session(&mock, &cfg(), &inp).unwrap();
        let req = mock.requests.lock().unwrap();
        assert_eq!(req.len(), 1);
        let body: serde_json::Value = serde_json::from_str(&req[0].1).unwrap();
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["system"][0]["text"], SYSTEM_PROMPT);
    }
}
