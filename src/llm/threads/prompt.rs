use super::super::common::{build_request_body_with_system, LlmConfig, LlmError};
use super::SessionDigest;

/// System prompt for thread topic clustering. Same caching contract as
/// `SYSTEM_PROMPT`: the byte sequence must be stable across calls in
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
{\"threads\":[{\"topic_summary\":\"<one short noun phrase>\",\"member_refs\":[\"<provider-slug>/<session-id> or <source>:<provider-slug>/<session-id>\"],\"time_span\":{\"start\":\"<RFC3339>\",\"end\":\"<RFC3339>\"}}]}

Rules:
- topic_summary: short noun phrase (\"BM25 search ranking\", \"thread clustering CLI\"). No verbs, no full sentence.
- member_refs: each entry must be a session ref that appears verbatim in the input. Do not invent refs or strip source prefixes.
- time_span.start: earliest started_at across the thread's members (copy from input).
- time_span.end: latest ended_at across the thread's members (copy from input; fall back to started_at if no ended_at given).
- Every input ref should appear in exactly one thread (singletons included).
- If the input is empty, return {\"threads\":[]}.";

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
            // Collapse newlines so each digest stays one line; the model
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
