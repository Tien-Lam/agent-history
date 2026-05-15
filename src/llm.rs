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

mod common;
mod decisions;
mod threads;
mod todos;
mod track;
#[cfg(test)]
use crate::model::{Provider, Role, SessionId};
#[cfg(test)]
use chrono::{DateTime, Utc};
#[cfg(test)]
use std::time::Duration;

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

    fn input<'a>(sid: &'a SessionId, candidates: Vec<Candidate<'a>>) -> ExtractionInput<'a> {
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
        let body = r#"{"id":"msg_x","content":[{"type":"tool_use","name":"x","input":{}}]}"#;
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
        let inner =
            r#"{"decisions":[{"summary":"Use BM25","rationale":"r","alternatives":[],"turn":7}]}"#;
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
        let mock = MockTransport::new(vec![(429, r#"{"error":"rate limited"}"#.to_string())]);
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
            digest(
                Provider::ClaudeCode,
                "a",
                Some("foo"),
                0,
                Some(60),
                Some("seed-foo"),
            ),
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
        let inner = "```json\n{\"threads\":[]}\n```";
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
        let mock = MockTransport::new(vec![(200, threads_assistant_response(inner))]);
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
        let mock = MockTransport::new(vec![(200, threads_assistant_response(inner))]);
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
        let mock = MockTransport::new(vec![(200, threads_assistant_response(inner))]);
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
        let mock = MockTransport::new(vec![(429, r#"{"error":"rate limited"}"#.to_string())]);
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
        let mock = MockTransport::new(vec![(200, threads_assistant_response(inner))]);
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
        let inner =
            r#"{"todos":[{"description":"Revisit BM25","raised_at":7,"status_inferred":"open"}]}"#;
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
        let inner =
            r#"{"todos":[{"description":"Revisit BM25","raised_at":7,"status_inferred":"open"}]}"#;
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
        assert!(
            out.is_empty(),
            "raised_at=0 must be rejected by CitationRef"
        );
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
                TodoCandidate {
                    turn: 1,
                    role: Role::Assistant,
                    kind: "todo",
                    snippet: "TODO see claude-code/abc-123",
                },
                TodoCandidate {
                    turn: 2,
                    role: Role::Assistant,
                    kind: "bd-ref",
                    snippet: "see ahist-y3o.7.2",
                },
                TodoCandidate {
                    turn: 3,
                    role: Role::Assistant,
                    kind: "todo",
                    snippet: "TODO unrelated",
                },
            ],
        );
        let out = extract_for_session_todos(&mock, &cfg(), &inp).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(
            out[0].todo.target_session.as_deref(),
            Some("claude-code/abc-123")
        );
        assert_eq!(out[1].todo.target_session.as_deref(), Some("ahist-y3o.7.2"));
        assert!(
            out[2].todo.target_session.is_none(),
            "bogus ref must be dropped"
        );
    }

    #[test]
    fn extract_for_session_todos_strips_turn_suffix_from_target_ref() {
        let inner = r#"{"todos":[{"description":"x","raised_at":1,"target_session":"claude-code/abc-123#7","status_inferred":"open"}]}"#;
        let mock = MockTransport::new(vec![(200, todos_assistant_response(inner))]);
        let sid = SessionId("s".into());
        let inp = todo_input(
            &sid,
            vec![TodoCandidate {
                turn: 1,
                role: Role::Assistant,
                kind: "todo",
                snippet: "TODO",
            }],
        );
        let out = extract_for_session_todos(&mock, &cfg(), &inp).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].todo.target_session.as_deref(),
            Some("claude-code/abc-123")
        );
    }

    #[test]
    fn extract_for_session_todos_propagates_api_errors() {
        let mock = MockTransport::new(vec![(429, r#"{"error":"rate limited"}"#.to_string())]);
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
            vec![TodoCandidate {
                turn: 1,
                role: Role::User,
                kind: "todo",
                snippet: "TODO",
            }],
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
