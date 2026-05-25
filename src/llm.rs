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
//! the system tokens. This is the main cost lever for the LLM-backed paths.
//!
//! ## Testability
//!
//! Network IO goes through the [`LlmTransport`] trait. Tests use
//! `MockTransport` to drive the extractor end-to-end without sockets.

mod common;
mod decisions;
mod threads;
mod todos;
mod track;

#[cfg(test)]
use common::extract_json_object;
pub use common::{LlmConfig, LlmError, LlmTransport, UreqTransport};
pub use decisions::{
    build_request_body, extract_for_session, parse_response, user_message, Candidate,
    ExtractedDecision, ExtractionInput, StructuredDecision, SYSTEM_PROMPT,
};
pub use threads::{
    build_threads_request_body, extract_threads, parse_threads_response, user_message_threads,
    SessionDigest, StructuredThread, TimeSpan, SYSTEM_PROMPT_THREADS,
};
#[cfg(test)]
use todos::sanitize_target_session;
pub use todos::{
    build_todos_request_body, extract_for_session_todos, parse_todos_response, user_message_todos,
    ExtractedTodo, StructuredTodo, TodoCandidate, TodoExtractionInput, TodoStatus,
    SYSTEM_PROMPT_TODOS,
};
pub use track::{
    build_track_request_body, extract_track, parse_track_response, user_message_track, TrackEvent,
    TrackSession, SYSTEM_PROMPT_TRACK,
};

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
mod tests;
