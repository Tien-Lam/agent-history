use super::super::test_support::MockTransport;
use super::super::*;
use super::{assistant_response, cfg};
use crate::model::{Provider, SessionId};
use chrono::{DateTime, Utc};

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
        source: None,
        provider,
        session_id: SessionId(id.to_string()),
        project: project.map(str::to_string),
        started_at: ts(start),
        ended_at: end.map(ts),
        summary: summary.map(str::to_string),
    }
}

#[test]
fn user_message_threads_preserves_remote_source_refs() {
    let digests = vec![SessionDigest {
        source: Some("laptop".to_string()),
        provider: Provider::ClaudeCode,
        session_id: SessionId("abc-123".to_string()),
        project: Some("alpha".to_string()),
        started_at: ts(0),
        ended_at: None,
        summary: None,
    }];

    let msg = user_message_threads(&digests);
    assert!(msg.contains("laptop:claude-code/abc-123"));
}

#[test]
fn user_message_track_preserves_remote_source_refs() {
    let sessions = vec![TrackSession {
        source: Some("laptop".to_string()),
        provider: Provider::ClaudeCode,
        session_id: SessionId("abc-123".to_string()),
        started_at: ts(0),
        excerpts: vec!["BM25 ranking changed".to_string()],
    }];

    let msg = user_message_track("BM25", &sessions);
    assert!(msg.contains("laptop:claude-code/abc-123"));
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
