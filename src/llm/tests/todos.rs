use super::super::test_support::MockTransport;
use super::super::*;
use super::{assistant_response, cfg};
use crate::model::{Provider, Role, SessionId};

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
fn sanitize_target_session_accepts_source_qualified_provider_slug() {
    assert_eq!(
        sanitize_target_session("laptop:claude-code/abc"),
        Some("laptop:claude-code/abc".to_string())
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
