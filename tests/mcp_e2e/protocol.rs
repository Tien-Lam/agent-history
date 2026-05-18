use serde_json::Value;

use crate::common;
use crate::common::mcp::run_session;

#[test]
fn mcp_initialize_advertises_protocol_and_tools() {
    let dir = tempfile::tempdir().unwrap();
    let responses = run_session(
        dir.path(),
        &[
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    );

    // Two responses: initialize + tools/list. The notification gets none.
    assert_eq!(responses.len(), 2, "got: {responses:#?}");

    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "aghist");

    assert_eq!(responses[1]["id"], 2);
    let tool_names: Vec<&str> = responses[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for expected in [
        "search_sessions",
        "list_sessions",
        "get_session",
        "get_message",
        "reindex",
        "health",
    ] {
        assert!(
            tool_names.contains(&expected),
            "tools/list missing {expected} in {tool_names:?}"
        );
    }
}

#[test]
fn mcp_list_sessions_with_no_data_returns_empty_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let responses = run_session(
        dir.path(),
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "list_sessions", "arguments": {} }
        })],
    );

    assert_eq!(responses.len(), 1);
    let result = &responses[0]["result"];
    assert_eq!(result["isError"], false);
    let structured = &result["structuredContent"];
    assert_eq!(structured["total"], 0);
    assert!(structured["sessions"].as_array().unwrap().is_empty());
}

#[test]
fn mcp_health_returns_structured_checks() {
    let dir = tempfile::tempdir().unwrap();
    let responses = run_session(
        dir.path(),
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "health", "arguments": {} }
        })],
    );

    assert_eq!(responses.len(), 1);
    let result = &responses[0]["result"];
    assert_eq!(result["isError"], false);
    let structured = &result["structuredContent"];
    let checks = structured["checks"].as_array().expect("checks array");
    let names: Vec<&str> = checks.iter().map(|c| c["name"].as_str().unwrap()).collect();
    for expected in [
        "providers-detected",
        "index-dir-writable",
        "manifest-sane",
        "index-schema-present",
    ] {
        assert!(
            names.contains(&expected),
            "health checks missing {expected} in {names:?}"
        );
    }
}

#[test]
fn mcp_resources_list_and_read_round_trip_against_claude_fixture() {
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();

    // 1. List resources, pick the first session URI off the wire (no string
    //    munging — we want to prove the URI we hand back resolves).
    let listings = run_session(
        home,
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/list"
        })],
    );
    let resources = listings[0]["result"]["resources"].as_array().unwrap();
    assert!(
        !resources.is_empty(),
        "expected at least one session resource"
    );
    let session_uri = resources[0]["uri"].as_str().unwrap().to_string();
    assert!(
        session_uri.starts_with("aghist://session/claude-code/"),
        "unexpected uri: {session_uri}"
    );
    assert_eq!(resources[0]["mimeType"], "application/json");

    // 2. Read the session resource — should return JSON content with all turns.
    let session_read = run_session(
        home,
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "resources/read",
            "params": { "uri": session_uri }
        })],
    );
    let contents = session_read[0]["result"]["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0]["mimeType"], "application/json");
    let body: Value = serde_json::from_str(contents[0]["text"].as_str().unwrap()).unwrap();
    let turns = body["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 4, "fixture has 4 messages, body: {body}");
    assert_eq!(turns[0]["turn"], 1);

    // 3. Read a per-turn URI directly and confirm it matches that turn.
    let turn_uri = turns[2]["uri"].as_str().unwrap().to_string();
    assert!(
        turn_uri.contains("/turn/3"),
        "expected /turn/3 in {turn_uri}"
    );
    let turn_read = run_session(
        home,
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "resources/read",
            "params": { "uri": turn_uri }
        })],
    );
    let body: Value = serde_json::from_str(
        turn_read[0]["result"]["contents"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["turn"]["turn"], 3);
    assert!(body["turn"]["ref"].as_str().unwrap().contains("#3"));
}

#[test]
fn mcp_resources_read_out_of_range_turn_is_invalid_params() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();

    // Find a real session id from list_sessions, then craft an out-of-range turn URI.
    let listings = run_session(
        home,
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/list"
        })],
    );
    let session_uri = listings[0]["result"]["resources"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    let bogus = format!("{session_uri}/turn/99");

    let resp = run_session(
        home,
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "resources/read",
            "params": { "uri": bogus }
        })],
    );
    assert_eq!(resp[0]["error"]["code"], -32602, "got: {:#?}", resp[0]);
    let msg = resp[0]["error"]["message"].as_str().unwrap();
    assert!(msg.contains("out of range"), "got: {msg}");
}

#[test]
fn mcp_list_sessions_finds_claude_fixture_via_provider_filter() {
    // Build a real provider on disk so we exercise the discover path through
    // tools/call rather than the no-providers shortcut.
    let fixture = common::fixtures::claude_single_session(3);
    let home = fixture.base_path.parent().unwrap();
    let responses = run_session(
        home,
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "list_sessions",
                "arguments": { "provider": "claude-code" }
            }
        })],
    );

    assert_eq!(responses.len(), 1);
    let structured = &responses[0]["result"]["structuredContent"];
    let sessions = structured["sessions"].as_array().unwrap();
    assert!(
        !sessions.is_empty(),
        "expected at least one session, got: {structured}"
    );
    assert_eq!(sessions[0]["provider"], "claude-code");
}
