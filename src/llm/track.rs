use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{
    build_request_body_with_system, post_request, response_json_object, LlmConfig, LlmError,
    LlmTransport,
};
use crate::model::{Provider, SessionId};

pub const SYSTEM_PROMPT_TRACK: &str = "\
You are a cross-session change tracker for a developer's AI-coding-agent history. \
Each input provides a topic and a set of sessions (in chronological order) that \
discuss it, with the relevant message excerpts from each session. Your task: \
identify what *changed* about the topic across sessions — decisions made, \
approaches revised, implementations shifted, ideas introduced or abandoned.

Schema, JSON only, no prose:
{\"timeline\":[{\"session_ref\":\"<provider-slug>/<session-id>\",\"date\":\"<YYYY-MM-DD>\",\"event\":\"<one-sentence description of what changed>\",\"direction\":\"introduced|revised|confirmed|dropped\"}]}

Rules:
- Each timeline entry describes one notable change or confirmation about the topic in that session.
- direction: \"introduced\" = first mention/implementation; \"revised\" = approach changed; \"confirmed\" = same approach reaffirmed; \"dropped\" = topic abandoned/reversed.
- event: one concrete sentence. Start with an action verb (\"Switched to...\", \"Added...\", \"Decided to...\", \"Dropped...\").
- session_ref: must match exactly one of the refs listed in the input. Do not invent refs.
- Only emit entries for sessions where something noteworthy happened about the topic. Skip sessions that merely mention the topic in passing.
- If no sessions have meaningful changes, return {\"timeline\":[]}.";

/// One session's topic-relevant excerpt for the track extractor.
#[derive(Debug, Clone)]
pub struct TrackSession {
    pub provider: Provider,
    pub session_id: SessionId,
    pub started_at: DateTime<Utc>,
    /// Relevant message snippets (up to 3, each up to 200 chars).
    pub excerpts: Vec<String>,
}

impl TrackSession {
    #[must_use]
    pub fn session_ref(&self) -> String {
        format!("{}/{}", self.provider.slug(), self.session_id.0)
    }
}

/// One event in the cross-session change timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackEvent {
    pub session_ref: String,
    pub date: String,
    pub event: String,
    pub direction: String,
}

/// Build user message for the track extractor.
#[must_use]
pub fn user_message_track(topic: &str, sessions: &[TrackSession]) -> String {
    let mut s = format!("Topic: {topic}\n\nSessions (oldest first):\n");
    for sess in sessions {
        use std::fmt::Write as _;
        let _ = writeln!(
            s,
            "\n[{}]  {}  ({})",
            sess.session_ref(),
            sess.started_at.format("%Y-%m-%d"),
            sess.started_at.format("%H:%M UTC"),
        );
        for (i, excerpt) in sess.excerpts.iter().enumerate() {
            let _ = writeln!(s, "  {}: {}", i + 1, excerpt);
        }
    }
    s
}

/// Build request body for the track extractor.
pub fn build_track_request_body(config: &LlmConfig, user: &str) -> Result<String, LlmError> {
    build_request_body_with_system(config, SYSTEM_PROMPT_TRACK, user)
}

#[derive(Deserialize)]
struct TrackPayload {
    timeline: Vec<TrackEvent>,
}

/// Parse the Messages API response into a timeline of change events.
pub fn parse_track_response(body: &str) -> Result<Vec<TrackEvent>, LlmError> {
    let json_slice = response_json_object(body)?;
    let parsed: TrackPayload = serde_json::from_str(&json_slice).map_err(|e| {
        LlmError::Parse(format!(
            "track payload: {e} (slice: {})",
            json_slice.chars().take(80).collect::<String>()
        ))
    })?;
    Ok(parsed.timeline)
}

/// Call the LLM and return a cross-session change timeline for `topic`.
pub fn extract_track<T: LlmTransport + ?Sized>(
    transport: &T,
    config: &LlmConfig,
    topic: &str,
    sessions: &[TrackSession],
) -> Result<Vec<TrackEvent>, LlmError> {
    if sessions.is_empty() {
        return Ok(Vec::new());
    }
    let user = user_message_track(topic, sessions);
    let body = build_track_request_body(config, &user)?;
    let resp_body = post_request(transport, config, &body)?;
    let valid_refs: std::collections::HashSet<String> =
        sessions.iter().map(TrackSession::session_ref).collect();
    let raw = parse_track_response(&resp_body)?;
    Ok(raw
        .into_iter()
        .filter(|e| valid_refs.contains(&e.session_ref))
        .collect())
}
