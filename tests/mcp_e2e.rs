mod common;

use std::io::Write;
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
    let mut child = Command::new(aghist_bin())
        .arg("mcp")
        .env("AGHIST_HOME", env_home)
        .env("AGHIST_INDEX_DIR", env_home.join("aghist-index"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn aghist mcp");

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
    let names: Vec<&str> = checks
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
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
