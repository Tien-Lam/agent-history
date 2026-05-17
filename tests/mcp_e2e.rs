mod common;

use serde_json::Value;

use common::mcp::{
    run_session, run_session_with_config, run_session_with_config_and_sources_cache,
};

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

#[test]
fn mcp_remote_source_cache_round_trips_list_search_message_and_resource() {
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("remote-mcp-session")
        .project("remote-mcp-proj")
        .user("REMOTE_MCP_TOKEN remote message body")
        .assistant("remote answer")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let responses = run_session_with_config_and_sources_cache(
        source.empty_home.path(),
        Some(&source.config_path),
        Some(&source.cache_dir),
        &[
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "list_sessions",
                    "arguments": { "provider": "claude-code" }
                }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "search_sessions",
                    "arguments": { "query": "REMOTE_MCP_TOKEN" }
                }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "resources/list"
            }),
        ],
    );

    let (session_uri, hit_ref) = assert_remote_mcp_initial_responses(&responses);
    let unqualified_hit_ref = hit_ref
        .strip_prefix("laptop:")
        .unwrap_or(&hit_ref)
        .to_string();

    let followup = run_session_with_config_and_sources_cache(
        source.empty_home.path(),
        Some(&source.config_path),
        Some(&source.cache_dir),
        &[
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "get_message",
                    "arguments": { "ref": hit_ref, "include_context": 1 }
                }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "resources/read",
                "params": { "uri": session_uri }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 6,
                "method": "tools/call",
                "params": {
                    "name": "get_message",
                    "arguments": { "ref": unqualified_hit_ref }
                }
            }),
        ],
    );

    assert_remote_mcp_followup(&followup, &hit_ref);
}

fn assert_remote_mcp_initial_responses(responses: &[Value]) -> (String, String) {
    let sessions = responses[0]["result"]["structuredContent"]["sessions"]
        .as_array()
        .unwrap();
    assert_eq!(sessions.len(), 1, "list response: {:#?}", responses[0]);
    assert_eq!(sessions[0]["source"], "laptop");
    assert_eq!(sessions[0]["provider"], "claude-code");
    let session_uri = sessions[0]["uri"].as_str().unwrap().to_string();
    assert!(
        session_uri.starts_with("aghist://source/laptop/session/claude-code/"),
        "unexpected remote session uri: {session_uri}"
    );

    let hits = responses[1]["result"]["structuredContent"]["hits"]
        .as_array()
        .unwrap();
    assert!(!hits.is_empty(), "search response: {:#?}", responses[1]);
    assert_eq!(hits[0]["source"], "laptop");
    let hit_ref = hits[0]["ref"].as_str().unwrap().to_string();
    assert!(
        hit_ref.starts_with("laptop:claude-code/remote-mcp-session#"),
        "unexpected remote ref: {hit_ref}"
    );

    let resources = responses[2]["result"]["resources"].as_array().unwrap();
    assert!(
        resources
            .iter()
            .any(|resource| resource["uri"].as_str() == Some(session_uri.as_str())),
        "resources/list should include source-qualified URI: {resources:#?}"
    );
    (session_uri, hit_ref)
}

fn assert_remote_mcp_followup(followup: &[Value], hit_ref: &str) {
    let message = &followup[0]["result"]["structuredContent"];
    assert_eq!(message["session"]["source"], "laptop");
    assert_eq!(message["ref"].as_str().unwrap(), hit_ref);
    assert!(
        message["turns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|turn| turn["is_target"].as_bool() == Some(true)),
        "expected target turn in get_message response: {message:#?}"
    );

    let contents = followup[1]["result"]["contents"].as_array().unwrap();
    let body: Value = serde_json::from_str(contents[0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["session"]["source"], "laptop");
    assert_eq!(body["turns"].as_array().unwrap().len(), 2);
    assert!(body["turns"][0]["uri"]
        .as_str()
        .unwrap()
        .starts_with("aghist://source/laptop/session/claude-code/"));

    let unqualified_message = &followup[2]["result"]["structuredContent"];
    assert_eq!(unqualified_message["session"]["source"], "laptop");
    assert_eq!(unqualified_message["ref"].as_str().unwrap(), hit_ref);
}

#[test]
fn mcp_exposed_subset_blocks_remote_source_providers() {
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("remote-hidden-mcp")
        .project("remote-hidden")
        .user("REMOTE_HIDDEN_MCP_TOKEN")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source_with_config(
        &remote.base_path,
        r#"
[providers]
mcp_exposed = ["copilot-cli"]
"#,
    );

    let responses = run_session_with_config_and_sources_cache(
        source.empty_home.path(),
        Some(&source.config_path),
        Some(&source.cache_dir),
        &[
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "list_sessions",
                    "arguments": { "provider": "claude-code" }
                }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "search_sessions",
                    "arguments": { "query": "REMOTE_HIDDEN_MCP_TOKEN" }
                }
            }),
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "resources/list"
            }),
        ],
    );

    let list_result = &responses[0]["result"];
    assert_eq!(list_result["isError"], false, "got: {list_result}");
    let listed = &list_result["structuredContent"];
    assert_eq!(listed["total"], 0, "hidden remote list leaked: {listed}");
    assert!(listed["sessions"].as_array().unwrap().is_empty());

    let search = &responses[1]["result"]["structuredContent"];
    assert_eq!(search["total"], 0, "hidden remote search leaked: {search}");
    assert!(search["hits"].as_array().unwrap().is_empty());

    let resources = responses[2]["result"]["resources"].as_array().unwrap();
    assert!(
        resources.is_empty(),
        "hidden remote provider leaked resources: {resources:#?}"
    );
}

#[test]
fn mcp_exposed_empty_hides_all_providers_from_mcp() {
    // Provider files exist on disk and would normally be discovered, but the
    // config opts out of exposing them via MCP. The server should report zero
    // sessions even though `aghist --list` would show them.
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    let config_path = home.join("aghist-config.toml");
    std::fs::write(&config_path, "[providers]\nmcp_exposed = []\n").unwrap();

    let responses = run_session_with_config(
        home,
        Some(&config_path),
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "list_sessions", "arguments": {} }
        })],
    );

    assert_eq!(responses.len(), 1);
    let structured = &responses[0]["result"]["structuredContent"];
    assert_eq!(structured["total"], 0);
    assert!(structured["sessions"].as_array().unwrap().is_empty());
}

#[test]
fn mcp_exposed_subset_blocks_unlisted_providers() {
    // Claude is on disk and enabled, but mcp_exposed only lets copilot through.
    // get_message must refuse to resolve a claude ref because the provider
    // isn't in the MCP-visible set, even though it's available locally.
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    let config_path = home.join("aghist-config.toml");
    std::fs::write(
        &config_path,
        "[providers]\nmcp_exposed = [\"copilot-cli\"]\n",
    )
    .unwrap();

    let responses = run_session_with_config(
        home,
        Some(&config_path),
        &[serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "get_message",
                "arguments": { "ref": "claude-code/session-gen#1" }
            }
        })],
    );

    assert_eq!(responses.len(), 1);
    let result = &responses[0]["result"];
    assert_eq!(result["isError"], true, "expected error, got: {result}");
    let txt = result["content"][0]["text"].as_str().unwrap();
    assert!(
        txt.contains("claude-code") && txt.contains("not enabled"),
        "expected provider-not-enabled message, got: {txt}"
    );
}

#[test]
fn mcp_exposed_unset_keeps_all_enabled_providers_visible() {
    // Sanity check that omitting mcp_exposed preserves prior behaviour: a
    // config that only sets unrelated fields shouldn't narrow MCP visibility.
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    let config_path = home.join("aghist-config.toml");
    std::fs::write(&config_path, "cache_size = 7\n").unwrap();

    let responses = run_session_with_config(
        home,
        Some(&config_path),
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

    let sessions = responses[0]["result"]["structuredContent"]["sessions"]
        .as_array()
        .unwrap();
    assert!(!sessions.is_empty());
}

#[test]
fn mcp_tool_calls_do_not_mutate_provider_history() {
    // Read-only contract: every tool exposed by the MCP server must leave the
    // upstream session files untouched. We snapshot every file under the
    // provider's base directory before/after a representative battery of tool
    // calls and assert byte-for-byte equality.
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();

    let before = common::mcp::snapshot_tree(&fixture.base_path);

    let responses = run_session(
        home,
        &[
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            serde_json::json!({
                "jsonrpc":"2.0","id":2,"method":"tools/call",
                "params":{"name":"list_sessions","arguments":{"provider":"claude-code"}}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":3,"method":"tools/call",
                "params":{"name":"search_sessions","arguments":{"query":"User"}}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":4,"method":"tools/call",
                "params":{"name":"get_session","arguments":{"session_id":"session-gen"}}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":5,"method":"tools/call",
                "params":{"name":"get_message","arguments":{"ref":"claude-code/session-gen#1"}}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":6,"method":"tools/call",
                "params":{"name":"reindex","arguments":{"force":true}}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":7,"method":"tools/call",
                "params":{"name":"health","arguments":{}}
            }),
            serde_json::json!({"jsonrpc":"2.0","id":8,"method":"resources/list"}),
            serde_json::json!({
                "jsonrpc":"2.0","id":9,"method":"resources/read",
                "params":{"uri":"aghist://session/claude-code/session-gen"}
            }),
            serde_json::json!({
                "jsonrpc":"2.0","id":10,"method":"resources/read",
                "params":{"uri":"aghist://session/claude-code/session-gen/turn/1"}
            }),
        ],
    );
    // Every call should have succeeded — otherwise we'd be asserting "no
    // mutation" on a code path that never ran.
    for resp in &responses {
        if let Some(result) = resp.get("result") {
            if let Some(is_error) = result.get("isError") {
                assert_eq!(is_error, &Value::Bool(false), "tool errored: {resp}");
            }
        }
    }

    let after = common::mcp::snapshot_tree(&fixture.base_path);
    assert_eq!(
        before, after,
        "MCP tool calls mutated provider history files (read-only contract violated)"
    );
}
