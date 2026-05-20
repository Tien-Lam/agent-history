use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use serde_json::json;

use crate::dto::{
    CursorMeta, ListEnvelope, McpListResponse, McpSearchResponse, McpSessionRow, MessageRow,
    SearchEnvelope, SearchHitJson, SearchMeta, SessionRow,
};
use crate::federated::SourceError;
use crate::model::{ContentBlock, Message, MessageId, Provider, Role, Session, SessionId};

use super::*;

#[test]
fn dto_schema_fragments_cover_serialized_keys() {
    let session = sample_session();
    assert_list_dto_schema_fragments(&session);
    assert_message_dto_schema_fragments();
    assert_search_dto_schema_fragments();
}

fn assert_list_dto_schema_fragments(session: &Session) {
    assert_schema_covers_serialized_keys(
        &crate::schema_fragments::session_row_schema(),
        &serde_json::to_value(SessionRow::from_session(session, "local")).unwrap(),
    );
    let mcp_session =
        McpSessionRow::from_session(session, "local", "aghist://local/claude-code/session-1");
    assert_schema_covers_serialized_keys(
        &crate::schema_fragments::mcp_session_row_schema(),
        &serde_json::to_value(mcp_session.clone()).unwrap(),
    );
    assert_schema_covers_serialized_keys(
        &crate::schema_fragments::list_response_schema(),
        &serde_json::to_value(ListEnvelope {
            sessions: vec![SessionRow::from_session(session, "local")],
            meta: CursorMeta::new(1, Some("cursor-1")),
        })
        .unwrap(),
    );
    assert_schema_covers_serialized_keys(
        &crate::schema_fragments::mcp_list_response_schema(),
        &serde_json::to_value(McpListResponse {
            total: 1,
            sessions: vec![mcp_session],
            source_errors: vec![sample_source_error()],
        })
        .unwrap(),
    );
}

fn assert_message_dto_schema_fragments() {
    let message = sample_message();
    assert_schema_covers_serialized_keys(
        &crate::schema_fragments::message_row_schema(),
        &serde_json::to_value(
            MessageRow::from_message(
                &message,
                "local",
                1,
                Some("claude-code/session-1#1".to_string()),
                "aghist://local/claude-code/session-1/turns/1",
            )
            .with_target(true),
        )
        .unwrap(),
    );
}

fn assert_search_dto_schema_fragments() {
    let message_hit = sample_message_hit();
    let note_hit = sample_note_hit();
    assert_schema_covers_serialized_keys(
        &crate::schema_fragments::search_hit_schema(),
        &serde_json::to_value(message_hit.clone()).unwrap(),
    );
    assert_schema_covers_serialized_keys(
        &crate::schema_fragments::search_hit_schema(),
        &serde_json::to_value(note_hit).unwrap(),
    );
    assert_schema_covers_serialized_keys(
        &crate::schema_fragments::search_response_schema(),
        &serde_json::to_value(SearchEnvelope {
            hits: vec![message_hit.clone()],
            meta: SearchMeta::new(1, None, "lexical"),
        })
        .unwrap(),
    );
    assert_schema_covers_serialized_keys(
        &crate::schema_fragments::mcp_search_response_schema(),
        &serde_json::to_value(McpSearchResponse {
            query: "needle".to_string(),
            limit: 20,
            total: 1,
            hits: vec![message_hit],
            source_errors: vec![sample_source_error()],
        })
        .unwrap(),
    );
}

fn sample_message_hit() -> SearchHitJson {
    let started_at = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    SearchHitJson {
        kind: "message",
        session_id: "session-1".to_string(),
        message_id: "message-1".to_string(),
        score: 1.0,
        snippet: "snippet".to_string(),
        provider: Some(Provider::ClaudeCode),
        project: Some("project".to_string()),
        started_at: Some(started_at),
        source: "local".to_string(),
        note_id: None,
        ref_: Some("claude-code/session-1#1".to_string()),
        turn: Some(1),
        explanation: Some(json!({ "value": 1.0 })),
    }
}

fn sample_note_hit() -> SearchHitJson {
    SearchHitJson {
        kind: "note",
        session_id: String::new(),
        message_id: String::new(),
        score: 1.0,
        snippet: "note".to_string(),
        provider: None,
        project: None,
        started_at: None,
        source: "local".to_string(),
        note_id: Some(42),
        ref_: Some("claude-code/session-1".to_string()),
        turn: None,
        explanation: None,
    }
}

fn sample_source_error() -> SourceError {
    SourceError {
        source: "remote".to_string(),
        error: "missing".to_string(),
    }
}

fn sample_session() -> Session {
    Session {
        id: SessionId("session-1".to_string()),
        provider: Provider::ClaudeCode,
        project_path: Some(PathBuf::from("/tmp/project")),
        project_name: Some("project".to_string()),
        git_branch: Some("main".to_string()),
        started_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        ended_at: None,
        summary: Some("summary".to_string()),
        model: Some("model".to_string()),
        token_usage: None,
        message_count: 2,
        source_path: PathBuf::from("/tmp/project/session.jsonl"),
    }
}

fn sample_message() -> Message {
    Message {
        id: MessageId("message-1".to_string()),
        role: Role::Assistant,
        timestamp: Utc.with_ymd_and_hms(2026, 1, 1, 0, 1, 0).unwrap(),
        content: vec![ContentBlock::Text("hello".to_string())],
        model: Some("model".to_string()),
        token_usage: None,
    }
}

fn assert_schema_covers_serialized_keys(schema: &Value, value: &Value) {
    let properties = schema["properties"]
        .as_object()
        .expect("schema has properties object");
    let object = value.as_object().expect("serialized DTO is an object");
    for key in object.keys() {
        assert!(
            properties.contains_key(key),
            "schema missing property for serialized key {key}: {schema:#}"
        );
    }
}
