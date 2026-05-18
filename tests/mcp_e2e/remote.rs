use serde_json::Value;

use crate::common;
use crate::common::mcp::run_session_with_config_and_sources_cache;

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
