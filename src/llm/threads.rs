use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{post_request, LlmConfig, LlmError, LlmTransport};
use crate::model::{Provider, SessionId};

mod prompt;
mod response;

pub use prompt::{build_threads_request_body, user_message_threads, SYSTEM_PROMPT_THREADS};
pub use response::parse_threads_response;

/// One session as seen by the threads extractor. Compact on purpose:
/// digests dominate the input token count, so we send only what the LLM
/// needs to topic-cluster (ref, project, time range, optional summary).
#[derive(Debug, Clone)]
pub struct SessionDigest {
    /// Registered remote source name. `None` means local host history.
    pub source: Option<String>,
    pub provider: Provider,
    pub session_id: SessionId,
    pub project: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    /// Short label hint — usually the session's `summary` field. May be empty.
    pub summary: Option<String>,
}

impl SessionDigest {
    /// `<provider-slug>/<session-id>` for local history, or
    /// `<source>:<provider-slug>/<session-id>` for remote source history.
    /// This is the round-trip identifier used in `member_refs`.
    #[must_use]
    pub fn session_ref(&self) -> String {
        let raw = format!("{}/{}", self.provider.slug(), self.session_id.0);
        match self.source.as_deref().filter(|source| !source.is_empty()) {
            Some(source) => format!("{source}:{raw}"),
            None => raw,
        }
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
