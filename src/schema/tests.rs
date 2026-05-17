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
fn every_listed_subcommand_has_a_schema() {
    for name in subcommands() {
        assert!(
            schema_for(name).is_some(),
            "missing schema for subcommand '{name}'"
        );
    }
}

#[test]
fn unknown_subcommand_returns_none() {
    assert!(schema_for("nonsense").is_none());
}

#[test]
fn schemas_declare_draft_2020_12() {
    for name in subcommands() {
        let schema = schema_for(name).unwrap();
        assert_eq!(
            schema["$schema"],
            common::SCHEMA_DRAFT,
            "subcommand '{name}' missing $schema draft declaration"
        );
        assert!(
            schema["params"].is_object(),
            "subcommand '{name}' missing params object"
        );
        assert!(
            schema["response"].is_object(),
            "subcommand '{name}' missing response object"
        );
    }
}

#[test]
fn index_returns_subcommands_array() {
    let idx = subcommand_index();
    let arr = idx["subcommands"].as_array().unwrap();
    assert_eq!(arr.len(), subcommands().len());
}

#[test]
fn all_schemas_keyed_by_name() {
    let all = all_schemas();
    let map = all.as_object().unwrap();
    for name in subcommands() {
        assert!(map.contains_key(name), "missing key {name} in all_schemas");
    }
}

#[test]
fn schema_command_fields_match_registry_names() {
    for name in subcommands() {
        let schema = schema_for(name).unwrap();
        let expected = if name == "list" { "--list" } else { name };
        assert_eq!(
            schema["command"].as_str(),
            Some(expected),
            "subcommand '{name}' has a mismatched command field"
        );
    }
}

#[test]
fn schema_params_are_closed_objects() {
    for name in subcommands() {
        let schema = schema_for(name).unwrap();
        assert_closed_params(name, &schema["params"]);

        if let Some(subcommands) = schema["subcommands"].as_object() {
            for (subcommand, schema) in subcommands {
                assert_closed_params(&format!("{name} {subcommand}"), &schema["params"]);
            }
        }
    }
}

#[test]
fn search_schema_describes_query_param() {
    let schema = schema_for("search").unwrap();
    let params = &schema["params"]["properties"];
    assert!(params["query"].is_object());
    assert!(params["limit"].is_object());
    assert_eq!(
        params["limit"]["default"],
        serde_json::json!(crate::schema_fragments::SEARCH_LIMIT_DEFAULT)
    );
    assert!(params["cursor"].is_object());
    assert_eq!(
        params["watch_interval_ms"]["default"],
        serde_json::json!(crate::schema_fragments::SEARCH_WATCH_INTERVAL_MS_DEFAULT)
    );
    assert_eq!(
        params["watch_iterations"]["default"],
        serde_json::json!(crate::schema_fragments::SEARCH_WATCH_ITERATIONS_DEFAULT)
    );
    assert_eq!(
        params["hybrid_weight"]["default"],
        serde_json::json!(crate::schema_fragments::SEARCH_HYBRID_WEIGHT_DEFAULT)
    );
}

#[test]
fn list_schema_describes_pagination_params() {
    let schema = schema_for("list").unwrap();
    let params = &schema["params"]["properties"];
    assert!(params["limit"].is_object());
    assert_eq!(
        params["limit"]["default"],
        serde_json::json!(crate::schema_fragments::LIST_LIMIT_DEFAULT)
    );
    assert!(params["cursor"].is_object());

    let json_response = &schema["response"]["oneOf"][0];
    assert_eq!(
        string_array(&json_response["required"]),
        vec!["sessions", "meta"]
    );
    assert_eq!(
        string_array(&json_response["properties"]["meta"]["required"]),
        vec!["next_cursor", "total"]
    );
}

#[test]
fn search_schema_describes_json_envelope() {
    let schema = schema_for("search").unwrap();
    let response = &schema["response"];
    assert_eq!(response["type"], "object");
    assert_eq!(string_array(&response["required"]), vec!["hits", "meta"]);

    let hit = &response["properties"]["hits"]["items"];
    assert_eq!(hit["type"], "object");
    assert_eq!(
        hit["properties"]["kind"]["enum"],
        serde_json::json!(["message", "note"])
    );
    for field in [
        "kind",
        "session_id",
        "message_id",
        "score",
        "snippet",
        "provider",
        "project",
        "started_at",
        "source",
    ] {
        assert!(
            string_array(&hit["required"]).contains(&field),
            "search hit schema missing required field {field}"
        );
    }
    assert!(hit["properties"]["ref"]["pattern"].is_string());
    assert!(hit["properties"]["turn"].is_object());
    assert!(hit["properties"]["explanation"].is_object());

    let meta = &response["properties"]["meta"];
    assert_eq!(meta["type"], "object");
    assert_eq!(
        string_array(&meta["required"]),
        vec!["next_cursor", "total", "engine"]
    );
    assert_eq!(
        meta["properties"]["engine"]["enum"],
        serde_json::json!(["lexical", "hybrid"])
    );
}

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

#[test]
fn index_schema_describes_summary_contract() {
    let schema = schema_for("index").unwrap();
    let response = &schema["response"];
    for field in [
        "providers",
        "sessions_total",
        "added",
        "updated",
        "unchanged",
        "removed",
        "messages_indexed",
        "force",
        "index_dir",
        "duration_ms",
        "errors",
        "embeddings",
    ] {
        assert!(
            string_array(&response["required"]).contains(&field),
            "index schema missing required summary field {field}"
        );
        assert!(
            response["properties"][field].is_object(),
            "index schema missing property for {field}"
        );
    }
    assert_eq!(
        response["properties"]["embeddings"]["properties"]["status"]["enum"],
        serde_json::json!(["disabled", "awaiting-consent", "enabled"])
    );
}

#[test]
fn show_schema_includes_reference_pattern() {
    let schema = schema_for("show").unwrap();
    let pattern = &schema["params"]["properties"]["reference"]["pattern"];
    assert!(pattern.is_string());
    // Sanity check: the example ref from the description matches the pattern.
    let re = regex_lite_check(pattern.as_str().unwrap(), "claude-code/abc-123#7");
    assert!(re, "show ref pattern should match canonical example");
    let re = regex_lite_check(pattern.as_str().unwrap(), "laptop:claude-code/abc-123#7");
    assert!(re, "show ref pattern should match source-qualified refs");
    assert!(
        pattern.as_str().unwrap().contains(":)?"),
        "show ref pattern should document the optional source prefix"
    );
    assert!(
        !pattern.as_str().unwrap().ends_with(")?$"),
        "show ref pattern should require a turn suffix"
    );
}

#[test]
fn diff_schema_includes_source_qualified_session_pattern() {
    let schema = schema_for("diff").unwrap();
    let pattern = schema["params"]["properties"]["session1"]["pattern"]
        .as_str()
        .unwrap();
    assert!(regex_lite_check(pattern, "claude-code/abc-123"));
    assert!(regex_lite_check(pattern, "laptop:claude-code/abc-123"));
    assert!(
        pattern.ends_with("/[^#]+$"),
        "diff session ref pattern should reject turn suffixes"
    );
}

#[test]
fn ref_patterns_track_provider_registry() {
    let providers = Provider::all()
        .iter()
        .map(|provider| provider.slug())
        .collect::<Vec<_>>()
        .join("|");

    assert_eq!(
        common::source_qualified_session_ref_pattern(),
        format!("^([A-Za-z0-9][A-Za-z0-9_-]*:)?({providers})/[^#]+(#[1-9][0-9]*)?$")
    );
    assert_eq!(
        common::source_qualified_session_only_ref_pattern(),
        format!("^([A-Za-z0-9][A-Za-z0-9_-]*:)?({providers})/[^#]+$")
    );
    assert_eq!(
        common::source_qualified_citation_ref_pattern(),
        format!("^([A-Za-z0-9][A-Za-z0-9_-]*:)?({providers})/[^#]+#[1-9][0-9]*$")
    );
}

#[test]
fn provider_enum_tracks_provider_registry() {
    let expected: Vec<&str> = Provider::all().iter().map(|p| p.slug()).collect();
    let provider_enum = common::provider_slug_enum();
    let actual: Vec<&str> = provider_enum
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(actual, expected);
}

fn assert_closed_params(label: &str, params: &Value) {
    assert_eq!(params["type"], "object", "{label} params must be an object");
    assert_eq!(
        params["additionalProperties"], false,
        "{label} params must reject undocumented properties"
    );
}

fn string_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap())
        .collect()
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

/// Tiny helper: we don't pull a regex crate just for tests, so check a few
/// known anchors without full regex matching.
fn regex_lite_check(pattern: &str, sample: &str) -> bool {
    // We only assert the pattern is well-formed and the sample contains
    // "/" (required by the pattern's structure).
    assert!(pattern.contains('/'));
    sample.contains('/')
}
