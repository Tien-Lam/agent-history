use super::super::test_support::MockTransport;
use super::super::*;
use super::{assistant_response, cfg};
use crate::model::{Provider, Role, SessionId};

fn input<'a>(sid: &'a SessionId, candidates: Vec<Candidate<'a>>) -> ExtractionInput<'a> {
    ExtractionInput {
        provider: Provider::ClaudeCode,
        session_id: sid,
        project: Some("aghist"),
        candidates,
    }
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
