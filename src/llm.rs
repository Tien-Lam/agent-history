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

use chrono::{DateTime, Utc};
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
    build_request_body_with_system(config, SYSTEM_PROMPT, user)
}

/// Like [`build_request_body`] but with a caller-supplied system prompt.
/// The threads extractor uses this; keep both paths sharing the same
/// `cache_control: ephemeral` shape so the cache hit-rate logic is uniform.
fn build_request_body_with_system(
    config: &LlmConfig,
    system: &str,
    user: &str,
) -> Result<String, LlmError> {
    let req = Request {
        model: &config.model,
        max_tokens: config.max_tokens,
        system: [SystemBlock {
            kind: "text",
            text: system,
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

// ──────────────────────────────────────────────────────────────────────
// Threads extractor
//
// Where `decisions --llm` groups *sentences within a single session*,
// `threads --llm` groups *whole sessions across project boundaries* by
// semantic topic. The heuristic (`crate::threads`) buckets strictly by
// project+time-adjacency, so it can never join two sessions that worked
// on the same feature across different repo checkouts. The LLM path is
// the opt-in fix for that.
// ──────────────────────────────────────────────────────────────────────

/// System prompt for thread topic clustering. Same caching contract as
/// [`SYSTEM_PROMPT`] — the byte sequence must be stable across calls in
/// a single run so the prompt cache hits on call N>=2.
pub const SYSTEM_PROMPT_THREADS: &str = "\
You are a developer-history topic clusterer. Each input lists sessions \
from a developer's AI-coding-agent history, one session per line, with \
project, time range, and a short summary if available. Group sessions \
that worked on the same semantic topic — the same feature, bug, or \
refactor — even if they live in different projects or are days apart. \
Singleton sessions are fine; do not invent groupings just to use every \
input row.

Schema, JSON only, no prose:
{\"threads\":[{\"topic_summary\":\"<one short noun phrase>\",\"member_refs\":[\"<provider-slug>/<session-id>\"],\"time_span\":{\"start\":\"<RFC3339>\",\"end\":\"<RFC3339>\"}}]}

Rules:
- topic_summary: short noun phrase (\"BM25 search ranking\", \"thread clustering CLI\"). No verbs, no full sentence.
- member_refs: each entry must be a `provider/session-id` ref that appears verbatim in the input. Do not invent refs.
- time_span.start: earliest started_at across the thread's members (copy from input).
- time_span.end: latest ended_at across the thread's members (copy from input; fall back to started_at if no ended_at given).
- Every input ref should appear in exactly one thread (singletons included).
- If the input is empty, return {\"threads\":[]}.";

/// One session as seen by the threads extractor. Compact on purpose:
/// digests dominate the input token count, so we send only what the LLM
/// needs to topic-cluster (ref, project, time range, optional summary).
#[derive(Debug, Clone)]
pub struct SessionDigest {
    pub provider: Provider,
    pub session_id: SessionId,
    pub project: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    /// Short label hint — usually the session's `summary` field. May be empty.
    pub summary: Option<String>,
}

impl SessionDigest {
    /// `<provider-slug>/<session-id>` — the round-trip identifier used in
    /// `member_refs`. Matches the format produced by `crate::threads`.
    #[must_use]
    pub fn session_ref(&self) -> String {
        format!("{}/{}", self.provider.slug(), self.session_id.0)
    }
}

/// Inclusive time window for a thread, as returned by the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeSpan {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// One LLM-grouped thread.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredThread {
    pub topic_summary: String,
    pub member_refs: Vec<String>,
    pub time_span: TimeSpan,
}

/// Build the user-message body for the threads extractor. One line per
/// session keeps the format trivially parseable by the model and keeps
/// per-row token cost bounded.
#[must_use]
pub fn user_message_threads(digests: &[SessionDigest]) -> String {
    let mut s = String::from("Sessions:\n");
    for d in digests {
        s.push_str("- ");
        s.push_str(&d.session_ref());
        s.push_str(" | project=");
        s.push_str(d.project.as_deref().unwrap_or("(unknown)"));
        s.push_str(" | started_at=");
        s.push_str(&d.started_at.to_rfc3339());
        s.push_str(" | ended_at=");
        match d.ended_at {
            Some(end) => s.push_str(&end.to_rfc3339()),
            None => s.push_str("(none)"),
        }
        if let Some(summary) = d.summary.as_deref().filter(|x| !x.is_empty()) {
            s.push_str(" | summary=");
            // Collapse newlines so each digest stays one line — the model
            // looks at `\n` as a row delimiter.
            let mut buf = String::with_capacity(summary.len());
            for ch in summary.chars() {
                if ch == '\n' || ch == '\r' {
                    buf.push(' ');
                } else {
                    buf.push(ch);
                }
            }
            s.push_str(&buf);
        }
        s.push('\n');
    }
    s
}

/// Build the request body for a threads-extraction API call.
pub fn build_threads_request_body(
    config: &LlmConfig,
    user: &str,
) -> Result<String, LlmError> {
    build_request_body_with_system(config, SYSTEM_PROMPT_THREADS, user)
}

#[derive(Deserialize)]
struct ThreadsPayload {
    threads: Vec<StructuredThread>,
}

/// Parse the Messages API response body into structured threads. Same
/// JSON-extraction rules as [`parse_response`]: tolerate code fences and
/// trailing prose.
pub fn parse_threads_response(body: &str) -> Result<Vec<StructuredThread>, LlmError> {
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
    let parsed: ThreadsPayload = serde_json::from_str(json_slice).map_err(|e| {
        LlmError::Parse(format!(
            "threads payload: {e} (slice starts: {})",
            json_slice.chars().take(80).collect::<String>()
        ))
    })?;
    Ok(parsed.threads)
}

/// Run topic-clustering over all `digests` in a single API call. The
/// caller is responsible for capping `digests` to a manageable size —
/// see the `--llm-max-sessions` flag.
///
/// Threads whose `member_refs` reference sessions not present in
/// `digests` are dropped (the model occasionally invents refs). Empty
/// threads are dropped. Member refs are deduplicated in-order.
pub fn extract_threads<T: LlmTransport + ?Sized>(
    transport: &T,
    config: &LlmConfig,
    digests: &[SessionDigest],
) -> Result<Vec<StructuredThread>, LlmError> {
    if digests.is_empty() {
        return Ok(Vec::new());
    }
    let user = user_message_threads(digests);
    let body = build_threads_request_body(config, &user)?;
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
    let raw = parse_threads_response(&resp_body)?;
    let valid_refs: std::collections::HashSet<String> =
        digests.iter().map(SessionDigest::session_ref).collect();
    let mut out = Vec::with_capacity(raw.len());
    for mut t in raw {
        // Drop refs the model hallucinated; preserve first-seen order; dedup.
        let mut seen = std::collections::HashSet::new();
        t.member_refs.retain(|r| valid_refs.contains(r) && seen.insert(r.clone()));
        if t.member_refs.is_empty() {
            continue;
        }
        if t.topic_summary.trim().is_empty() {
            continue;
        }
        out.push(t);
    }
    Ok(out)
}

// ──────────────────────────────────────────────────────────────────────
// Todos extractor
//
// `todos --llm` routes the heuristic TODO/follow-up/bd-ref candidates from
// `crate::todos` through Claude. The heuristic emits raw matched lines
// (e.g. `// TODO: revisit BM25`); the LLM rewrites each surviving candidate
// into a structured record per the bead schema:
//
//     {description, raised_at: ref, target_session?, status_inferred}
//
// `target_session` is best-effort — the model may surface a `<provider>/<id>`
// or beads-style ref it sees inside the TODO snippet. We validate it is a
// well-formed citation/bd ref and drop it otherwise (no hallucinated refs).
// `status_inferred` is one of `open` / `done` / `unclear`.
// ──────────────────────────────────────────────────────────────────────

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
/// JSON-extraction tolerance as [`parse_response`].
pub fn parse_todos_response(body: &str) -> Result<Vec<StructuredTodo>, LlmError> {
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
    let parsed: TodosPayload = serde_json::from_str(json_slice).map_err(|e| {
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
fn sanitize_target_session(raw: &str) -> Option<String> {
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
/// [`extract_for_session`] for decisions: builds the request, posts it,
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

    // ── threads extractor ────────────────────────────────────────────

    fn ts(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(secs, 0).expect("valid ts")
    }

    fn digest(
        provider: Provider,
        id: &str,
        project: Option<&str>,
        start: i64,
        end: Option<i64>,
        summary: Option<&str>,
    ) -> SessionDigest {
        SessionDigest {
            provider,
            session_id: SessionId(id.to_string()),
            project: project.map(str::to_string),
            started_at: ts(start),
            ended_at: end.map(ts),
            summary: summary.map(str::to_string),
        }
    }

    fn threads_assistant_response(inner: &str) -> String {
        assistant_response(inner)
    }

    #[test]
    fn user_message_threads_one_line_per_session_with_project_and_times() {
        let digests = vec![
            digest(Provider::ClaudeCode, "a", Some("foo"), 0, Some(60), Some("seed-foo")),
            digest(Provider::CodexCli, "b", None, 120, None, None),
        ];
        let msg = user_message_threads(&digests);
        let lines: Vec<&str> = msg.lines().collect();
        assert_eq!(lines[0], "Sessions:");
        assert!(lines[1].contains("- claude-code/a"));
        assert!(lines[1].contains("project=foo"));
        assert!(lines[1].contains("ended_at=1970-01-01T00:01:00"));
        assert!(lines[1].contains("summary=seed-foo"));
        assert!(lines[2].contains("- codex-cli/b"));
        assert!(lines[2].contains("project=(unknown)"));
        assert!(lines[2].contains("ended_at=(none)"));
        assert!(!lines[2].contains("summary="));
    }

    #[test]
    fn user_message_threads_strips_embedded_newlines_in_summary() {
        let digests = vec![digest(
            Provider::ClaudeCode,
            "a",
            Some("foo"),
            0,
            None,
            Some("line one\nline two\rline three"),
        )];
        let msg = user_message_threads(&digests);
        // One session-line; newlines must not split the row.
        let body_lines: Vec<&str> = msg.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(body_lines.len(), 1);
        assert!(body_lines[0].contains("line one line two line three"));
    }

    #[test]
    fn build_threads_request_body_includes_cache_control_on_threads_prompt() {
        let body = build_threads_request_body(&cfg(), "Sessions:\n").unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(v["system"][0]["text"], SYSTEM_PROMPT_THREADS);
        assert_eq!(v["messages"][0]["content"], "Sessions:\n");
    }

    #[test]
    fn parse_threads_response_handles_plain_json() {
        let inner = r#"{"threads":[{"topic_summary":"BM25 ranking","member_refs":["claude-code/a","codex-cli/b"],"time_span":{"start":"2026-01-01T00:00:00Z","end":"2026-01-02T00:00:00Z"}}]}"#;
        let body = threads_assistant_response(inner);
        let out = parse_threads_response(&body).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].topic_summary, "BM25 ranking");
        assert_eq!(out[0].member_refs, vec!["claude-code/a", "codex-cli/b"]);
        assert_eq!(
            out[0].time_span,
            TimeSpan {
                start: "2026-01-01T00:00:00Z".parse().unwrap(),
                end: "2026-01-02T00:00:00Z".parse().unwrap(),
            }
        );
    }

    #[test]
    fn parse_threads_response_handles_code_fenced_json() {
        let inner =
            "```json\n{\"threads\":[]}\n```";
        let body = threads_assistant_response(inner);
        let out = parse_threads_response(&body).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn extract_threads_skips_when_no_digests() {
        let mock = MockTransport::new(vec![]);
        let out = extract_threads(&mock, &cfg(), &[]).unwrap();
        assert!(out.is_empty());
        assert!(mock.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn extract_threads_drops_hallucinated_member_refs() {
        let digests = vec![
            digest(Provider::ClaudeCode, "a", Some("foo"), 0, None, None),
            digest(Provider::ClaudeCode, "b", Some("foo"), 60, None, None),
        ];
        // Model returns one valid ref plus one invented one; we keep the
        // valid ref and drop the rest.
        let inner = r#"{"threads":[{"topic_summary":"feature x","member_refs":["claude-code/a","claude-code/ghost"],"time_span":{"start":"1970-01-01T00:00:00Z","end":"1970-01-01T00:01:00Z"}}]}"#;
        let mock =
            MockTransport::new(vec![(200, threads_assistant_response(inner))]);
        let out = extract_threads(&mock, &cfg(), &digests).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].member_refs, vec!["claude-code/a"]);
    }

    #[test]
    fn extract_threads_drops_thread_with_no_remaining_members() {
        let digests = vec![digest(
            Provider::ClaudeCode,
            "a",
            Some("foo"),
            0,
            None,
            None,
        )];
        let inner = r#"{"threads":[{"topic_summary":"all-hallucinated","member_refs":["claude-code/ghost1","claude-code/ghost2"],"time_span":{"start":"1970-01-01T00:00:00Z","end":"1970-01-01T00:00:00Z"}}]}"#;
        let mock =
            MockTransport::new(vec![(200, threads_assistant_response(inner))]);
        let out = extract_threads(&mock, &cfg(), &digests).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn extract_threads_dedups_member_refs_preserving_order() {
        let digests = vec![
            digest(Provider::ClaudeCode, "a", Some("foo"), 0, None, None),
            digest(Provider::ClaudeCode, "b", Some("foo"), 60, None, None),
        ];
        let inner = r#"{"threads":[{"topic_summary":"dup","member_refs":["claude-code/b","claude-code/a","claude-code/b"],"time_span":{"start":"1970-01-01T00:00:00Z","end":"1970-01-01T00:01:00Z"}}]}"#;
        let mock =
            MockTransport::new(vec![(200, threads_assistant_response(inner))]);
        let out = extract_threads(&mock, &cfg(), &digests).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].member_refs, vec!["claude-code/b", "claude-code/a"]);
    }

    #[test]
    fn extract_threads_propagates_api_errors() {
        let digests = vec![digest(
            Provider::ClaudeCode,
            "a",
            Some("foo"),
            0,
            None,
            None,
        )];
        let mock =
            MockTransport::new(vec![(429, r#"{"error":"rate limited"}"#.to_string())]);
        let err = extract_threads(&mock, &cfg(), &digests).unwrap_err();
        match err {
            LlmError::ApiStatus { status, .. } => assert_eq!(status, 429),
            other => panic!("expected ApiStatus, got {other:?}"),
        }
    }

    #[test]
    fn extract_threads_sends_correct_headers_and_caches_system() {
        let inner = r#"{"threads":[]}"#;
        let digests = vec![digest(
            Provider::ClaudeCode,
            "a",
            Some("foo"),
            0,
            None,
            None,
        )];
        let mock = MockTransport::new(vec![(200, threads_assistant_response(inner))]);
        let _ = extract_threads(&mock, &cfg(), &digests).unwrap();
        let req = mock.requests.lock().unwrap();
        assert_eq!(req.len(), 1);
        let body: serde_json::Value = serde_json::from_str(&req[0].1).unwrap();
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["system"][0]["text"], SYSTEM_PROMPT_THREADS);
    }

    #[test]
    fn extract_threads_drops_thread_with_empty_topic_summary() {
        let digests = vec![digest(
            Provider::ClaudeCode,
            "a",
            Some("foo"),
            0,
            None,
            None,
        )];
        let inner = r#"{"threads":[{"topic_summary":"   ","member_refs":["claude-code/a"],"time_span":{"start":"1970-01-01T00:00:00Z","end":"1970-01-01T00:00:00Z"}}]}"#;
        let mock =
            MockTransport::new(vec![(200, threads_assistant_response(inner))]);
        let out = extract_threads(&mock, &cfg(), &digests).unwrap();
        assert!(out.is_empty());
    }

    // ── todos extractor ─────────────────────────────────────────────

    fn todo_input<'a>(
        sid: &'a SessionId,
        candidates: Vec<TodoCandidate<'a>>,
    ) -> TodoExtractionInput<'a> {
        TodoExtractionInput {
            provider: Provider::ClaudeCode,
            session_id: sid,
            project: Some("aghist"),
            candidates,
        }
    }

    fn todos_assistant_response(inner: &str) -> String {
        assistant_response(inner)
    }

    #[test]
    fn user_message_todos_includes_project_session_and_candidates() {
        let sid = SessionId("ses_abc".into());
        let inp = todo_input(
            &sid,
            vec![TodoCandidate {
                turn: 5,
                role: Role::Assistant,
                kind: "todo",
                snippet: "TODO: revisit BM25",
            }],
        );
        let msg = user_message_todos(&inp);
        assert!(msg.contains("Project: aghist"));
        assert!(msg.contains("Session: claude-code/ses_abc"));
        assert!(msg.contains("- turn 5 (assistant, kind=todo): TODO: revisit BM25"));
    }

    #[test]
    fn user_message_todos_strips_embedded_newlines_in_snippet() {
        let sid = SessionId("s".into());
        let inp = todo_input(
            &sid,
            vec![TodoCandidate {
                turn: 1,
                role: Role::User,
                kind: "todo",
                snippet: "TODO\nmulti\rline",
            }],
        );
        let msg = user_message_todos(&inp);
        let body_lines: Vec<&str> = msg.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(body_lines.len(), 1);
        assert!(body_lines[0].contains("TODO multi line"));
    }

    #[test]
    fn build_todos_request_body_includes_cache_control_on_todos_prompt() {
        let body = build_todos_request_body(&cfg(), "x").unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(v["system"][0]["text"], SYSTEM_PROMPT_TODOS);
        assert_eq!(v["messages"][0]["content"], "x");
    }

    #[test]
    fn parse_todos_response_handles_plain_json() {
        let inner = r#"{"todos":[{"description":"Revisit BM25","raised_at":7,"status_inferred":"open"}]}"#;
        let body = todos_assistant_response(inner);
        let out = parse_todos_response(&body).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].description, "Revisit BM25");
        assert_eq!(out[0].raised_at, 7);
        assert_eq!(out[0].status_inferred, TodoStatus::Open);
        assert!(out[0].target_session.is_none());
    }

    #[test]
    fn parse_todos_response_handles_code_fenced_json() {
        let inner = "```json\n{\"todos\":[]}\n```";
        let body = todos_assistant_response(inner);
        let out = parse_todos_response(&body).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn parse_todos_response_errors_on_no_json() {
        let body = todos_assistant_response("the model refused");
        let err = parse_todos_response(&body).unwrap_err();
        assert!(matches!(err, LlmError::NoJson(_)));
    }

    #[test]
    fn extract_for_session_todos_skips_when_no_candidates() {
        let mock = MockTransport::new(vec![]);
        let sid = SessionId("ses".into());
        let inp = todo_input(&sid, vec![]);
        let out = extract_for_session_todos(&mock, &cfg(), &inp).unwrap();
        assert!(out.is_empty());
        assert!(mock.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn extract_for_session_todos_attaches_citation_and_source() {
        let inner = r#"{"todos":[{"description":"Revisit BM25","raised_at":7,"status_inferred":"open"}]}"#;
        let mock = MockTransport::new(vec![(200, todos_assistant_response(inner))]);
        let sid = SessionId("ses_abc".into());
        let inp = todo_input(
            &sid,
            vec![TodoCandidate {
                turn: 7,
                role: Role::Assistant,
                kind: "todo",
                snippet: "TODO revisit BM25",
            }],
        );
        let out = extract_for_session_todos(&mock, &cfg(), &inp).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].citation.to_string(), "claude-code/ses_abc#7");
        assert_eq!(out[0].source_snippet.as_deref(), Some("TODO revisit BM25"));
        assert_eq!(out[0].source_kind.as_deref(), Some("todo"));
        assert_eq!(out[0].todo.status_inferred, TodoStatus::Open);
    }

    #[test]
    fn extract_for_session_todos_drops_invalid_turn() {
        let inner = r#"{"todos":[{"description":"x","raised_at":0,"status_inferred":"open"}]}"#;
        let mock = MockTransport::new(vec![(200, todos_assistant_response(inner))]);
        let sid = SessionId("ses".into());
        let inp = todo_input(
            &sid,
            vec![TodoCandidate {
                turn: 1,
                role: Role::Assistant,
                kind: "todo",
                snippet: "anything",
            }],
        );
        let out = extract_for_session_todos(&mock, &cfg(), &inp).unwrap();
        assert!(out.is_empty(), "raised_at=0 must be rejected by CitationRef");
    }

    #[test]
    fn extract_for_session_todos_drops_blank_description() {
        let inner = r#"{"todos":[{"description":"   ","raised_at":1,"status_inferred":"open"}]}"#;
        let mock = MockTransport::new(vec![(200, todos_assistant_response(inner))]);
        let sid = SessionId("s".into());
        let inp = todo_input(
            &sid,
            vec![TodoCandidate {
                turn: 1,
                role: Role::User,
                kind: "todo",
                snippet: "TODO",
            }],
        );
        let out = extract_for_session_todos(&mock, &cfg(), &inp).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn extract_for_session_todos_keeps_valid_target_session_refs() {
        // citation-style ref + bd-style ref both valid; bogus string dropped.
        let inner = r#"{"todos":[
            {"description":"see other session","raised_at":1,"target_session":"claude-code/abc-123","status_inferred":"open"},
            {"description":"track in bd","raised_at":2,"target_session":"ahist-y3o.7.2","status_inferred":"unclear"},
            {"description":"hallucinated ref","raised_at":3,"target_session":"not-a-real-thing!!!","status_inferred":"open"}
        ]}"#;
        let mock = MockTransport::new(vec![(200, todos_assistant_response(inner))]);
        let sid = SessionId("s".into());
        let inp = todo_input(
            &sid,
            vec![
                TodoCandidate { turn: 1, role: Role::Assistant, kind: "todo", snippet: "TODO see claude-code/abc-123" },
                TodoCandidate { turn: 2, role: Role::Assistant, kind: "bd-ref", snippet: "see ahist-y3o.7.2" },
                TodoCandidate { turn: 3, role: Role::Assistant, kind: "todo", snippet: "TODO unrelated" },
            ],
        );
        let out = extract_for_session_todos(&mock, &cfg(), &inp).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].todo.target_session.as_deref(), Some("claude-code/abc-123"));
        assert_eq!(out[1].todo.target_session.as_deref(), Some("ahist-y3o.7.2"));
        assert!(out[2].todo.target_session.is_none(), "bogus ref must be dropped");
    }

    #[test]
    fn extract_for_session_todos_strips_turn_suffix_from_target_ref() {
        let inner = r#"{"todos":[{"description":"x","raised_at":1,"target_session":"claude-code/abc-123#7","status_inferred":"open"}]}"#;
        let mock = MockTransport::new(vec![(200, todos_assistant_response(inner))]);
        let sid = SessionId("s".into());
        let inp = todo_input(
            &sid,
            vec![TodoCandidate { turn: 1, role: Role::Assistant, kind: "todo", snippet: "TODO" }],
        );
        let out = extract_for_session_todos(&mock, &cfg(), &inp).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].todo.target_session.as_deref(), Some("claude-code/abc-123"));
    }

    #[test]
    fn extract_for_session_todos_propagates_api_errors() {
        let mock = MockTransport::new(vec![(429, r#"{"error":"rate limited"}"#.to_string())]);
        let sid = SessionId("s".into());
        let inp = todo_input(
            &sid,
            vec![TodoCandidate { turn: 1, role: Role::User, kind: "todo", snippet: "TODO" }],
        );
        let err = extract_for_session_todos(&mock, &cfg(), &inp).unwrap_err();
        match err {
            LlmError::ApiStatus { status, .. } => assert_eq!(status, 429),
            other => panic!("expected ApiStatus, got {other:?}"),
        }
    }

    #[test]
    fn extract_for_session_todos_sends_correct_headers_and_caches_system() {
        let inner = r#"{"todos":[]}"#;
        let mock = MockTransport::new(vec![(200, todos_assistant_response(inner))]);
        let sid = SessionId("s".into());
        let inp = todo_input(
            &sid,
            vec![TodoCandidate { turn: 1, role: Role::User, kind: "todo", snippet: "TODO" }],
        );
        let _ = extract_for_session_todos(&mock, &cfg(), &inp).unwrap();
        let req = mock.requests.lock().unwrap();
        assert_eq!(req.len(), 1);
        let body: serde_json::Value = serde_json::from_str(&req[0].1).unwrap();
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["system"][0]["text"], SYSTEM_PROMPT_TODOS);
    }

    #[test]
    fn sanitize_target_session_accepts_known_provider_slug() {
        assert_eq!(
            sanitize_target_session("claude-code/abc"),
            Some("claude-code/abc".to_string())
        );
    }

    #[test]
    fn sanitize_target_session_rejects_unknown_provider_slug() {
        assert!(sanitize_target_session("bogus-tool/x").is_none());
    }

    #[test]
    fn sanitize_target_session_accepts_bd_ref_with_digit_suffix() {
        assert_eq!(
            sanitize_target_session("ahist-y3o.7.2"),
            Some("ahist-y3o.7.2".to_string())
        );
    }

    #[test]
    fn sanitize_target_session_rejects_prose_hyphenate() {
        assert!(sanitize_target_session("follow-up").is_none());
    }
}
