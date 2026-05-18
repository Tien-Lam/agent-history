use crate::common;
use crate::common::mcp::{run_session_with_config, run_session_with_config_and_sources_cache};

#[test]
fn mcp_exposed_subset_blocks_remote_source_providers() {
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
    let fixture = common::fixtures::claude::claude_single_session(2);
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
    let fixture = common::fixtures::claude::claude_single_session(2);
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
    let fixture = common::fixtures::claude::claude_single_session(2);
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
