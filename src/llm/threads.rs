use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{
    build_request_body_with_system, post_request, response_json_object, LlmConfig, LlmError,
    LlmTransport,
};
use crate::model::{Provider, SessionId};

/// System prompt for thread topic clustering. Same caching contract as
/// `SYSTEM_PROMPT` — the byte sequence must be stable across calls in
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
pub fn build_threads_request_body(config: &LlmConfig, user: &str) -> Result<String, LlmError> {
    build_request_body_with_system(config, SYSTEM_PROMPT_THREADS, user)
}

#[derive(Deserialize)]
struct ThreadsPayload {
    threads: Vec<StructuredThread>,
}

/// Parse the Messages API response body into structured threads. Same
/// JSON-extraction rules as `parse_response`: tolerate code fences and
/// trailing prose.
pub fn parse_threads_response(body: &str) -> Result<Vec<StructuredThread>, LlmError> {
    let json_slice = response_json_object(body)?;
    let parsed: ThreadsPayload = serde_json::from_str(&json_slice).map_err(|e| {
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
    let resp_body = post_request(transport, config, &body)?;
    let raw = parse_threads_response(&resp_body)?;
    let valid_refs: std::collections::HashSet<String> =
        digests.iter().map(SessionDigest::session_ref).collect();
    let mut out = Vec::with_capacity(raw.len());
    for mut t in raw {
        // Drop refs the model hallucinated; preserve first-seen order; dedup.
        let mut seen = std::collections::HashSet::new();
        t.member_refs
            .retain(|r| valid_refs.contains(r) && seen.insert(r.clone()));
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
