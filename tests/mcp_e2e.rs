mod common;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

fn aghist_bin() -> std::path::PathBuf {
    // Resolve via assert_cmd so we exercise the same binary path tests use elsewhere.
    assert_cmd::cargo::cargo_bin("aghist")
}

/// Sends `requests` over stdin (one JSON-RPC line each) and returns the parsed
/// response objects in order. EOF on stdin tells the server to exit, so we
/// don't need an explicit `shutdown` call.
fn run_session(env_home: &std::path::Path, requests: &[Value]) -> Vec<Value> {
    run_session_with_config(env_home, None, requests)
}

fn run_session_with_config(
    env_home: &std::path::Path,
    config_path: Option<&Path>,
    requests: &[Value],
) -> Vec<Value> {
    let mut cmd = Command::new(aghist_bin());
    cmd.arg("mcp")
        .env("AGHIST_HOME", env_home)
        .env("AGHIST_INDEX_DIR", env_home.join("aghist-index"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(p) = config_path {
        cmd.env("AGHIST_CONFIG", p);
    }
    let mut child = cmd.spawn().expect("spawn aghist mcp");

    {
        let mut stdin = child.stdin.take().expect("child stdin");
        for req in requests {
            let line = serde_json::to_string(req).unwrap();
            stdin.write_all(line.as_bytes()).unwrap();
            stdin.write_all(b"\n").unwrap();
        }
        // Drop stdin → EOF → server exits.
    }

    let output = child.wait_with_output().expect("wait_with_output");
    assert!(
        output.status.success(),
        "aghist mcp exited non-zero: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");

    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("response is valid JSON"))
        .collect()
}

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

    let before = snapshot_tree(&fixture.base_path);

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

    let after = snapshot_tree(&fixture.base_path);
    assert_eq!(
        before, after,
        "MCP tool calls mutated provider history files (read-only contract violated)"
    );
}

/// Reads every regular file under `root` into a (relative-path → bytes) map.
/// Used by the read-only contract test to detect any change to provider files.
fn snapshot_tree(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    walk_into(root, root, &mut out);
    out
}

fn walk_into(
    root: &Path,
    dir: &Path,
    out: &mut std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>,
) {
    for entry in std::fs::read_dir(dir).expect("read provider dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        let ty = entry.file_type().expect("file type");
        if ty.is_symlink() {
            continue;
        }
        if ty.is_dir() {
            walk_into(root, &path, out);
        } else if ty.is_file() {
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            let bytes = std::fs::read(&path).expect("read provider file");
            out.insert(rel, bytes);
        }
    }
}
