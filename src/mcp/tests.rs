use std::io::Cursor;

use serde_json::Value;

use super::protocol::{ERR_INVALID_PARAMS, ERR_METHOD_NOT_FOUND, ERR_PARSE, PROTOCOL_VERSION};
use super::resources::{parse_aghist_uri, session_uri, turn_uri, ParsedUri};
use super::McpServer;
use crate::model::Provider;

fn server() -> McpServer {
    McpServer::new(Vec::new())
}

fn run_one(server: &McpServer, request: &str) -> Value {
    let input = format!("{request}\n");
    let mut output = Vec::new();
    server
        .serve(Cursor::new(input.as_bytes()), &mut output)
        .unwrap();
    let line = String::from_utf8(output).unwrap();
    serde_json::from_str(line.trim()).unwrap()
}

#[test]
fn initialize_returns_protocol_and_server_info() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(resp["result"]["serverInfo"]["name"], "aghist");
    assert!(resp["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn notification_produces_no_response() {
    let mut output = Vec::new();
    server()
        .serve(
            Cursor::new(
                b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n".as_slice(),
            ),
            &mut output,
        )
        .unwrap();
    assert!(output.is_empty(), "got unexpected response: {output:?}");
}

#[test]
fn tools_list_advertises_all_tools() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    );
    let names: Vec<&str> = resp["result"]["tools"]
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
            names.contains(&expected),
            "missing tool {expected} in {names:?}"
        );
    }
}

#[test]
fn unknown_method_returns_method_not_found() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":3,"method":"nope/nope"}"#,
    );
    assert_eq!(resp["error"]["code"], ERR_METHOD_NOT_FOUND);
}

#[test]
fn malformed_json_returns_parse_error() {
    let resp = run_one(&server(), "not json");
    assert_eq!(resp["error"]["code"], ERR_PARSE);
}

#[test]
fn unknown_tool_returns_tool_error_envelope() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"bogus","arguments":{}}}"#,
    );
    assert_eq!(resp["result"]["isError"], true);
    let txt = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(txt.contains("unknown tool"), "got: {txt}");
}

#[test]
fn list_sessions_with_no_providers_returns_empty() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_sessions","arguments":{}}}"#,
    );
    assert_eq!(resp["result"]["isError"], false);
    let structured = &resp["result"]["structuredContent"];
    assert_eq!(structured["total"], 0);
    assert!(structured["sessions"].as_array().unwrap().is_empty());
}

#[test]
fn get_message_with_invalid_ref_reports_error() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"get_message","arguments":{"ref":"not-a-ref"}}}"#,
    );
    assert_eq!(resp["result"]["isError"], true);
}

#[test]
fn get_message_with_missing_ref_arg_reports_error() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"get_message","arguments":{}}}"#,
    );
    assert_eq!(resp["result"]["isError"], true);
    let txt = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(txt.contains("ref"), "got: {txt}");
}

#[test]
fn list_sessions_validates_provider_slug() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"list_sessions","arguments":{"provider":"made-up"}}}"#,
    );
    assert_eq!(resp["result"]["isError"], true);
    let txt = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(txt.contains("made-up"), "got: {txt}");
}

#[test]
fn list_sessions_rejects_out_of_range_limit() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"list_sessions","arguments":{"limit":0}}}"#,
    );
    assert_eq!(resp["result"]["isError"], true);
}

#[test]
fn initialize_advertises_resources_capability() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    let caps = &resp["result"]["capabilities"];
    assert!(
        caps["resources"].is_object(),
        "missing resources cap: {caps}"
    );
    assert_eq!(caps["resources"]["subscribe"], false);
    assert_eq!(caps["resources"]["listChanged"], false);
}

#[test]
fn resources_list_with_no_providers_returns_empty() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#,
    );
    assert_eq!(resp["error"], Value::Null);
    assert!(resp["result"]["resources"].as_array().unwrap().is_empty());
}

#[test]
fn resources_templates_list_advertises_session_and_turn() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":1,"method":"resources/templates/list"}"#,
    );
    let templates = resp["result"]["resourceTemplates"].as_array().unwrap();
    let uris: Vec<&str> = templates
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();
    assert!(uris.contains(&"aghist://session/{provider}/{session_id}"));
    assert!(uris.contains(&"aghist://session/{provider}/{session_id}/turn/{turn}"));
}

#[test]
fn resources_read_missing_uri_is_invalid_params() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{}}"#,
    );
    assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
}

#[test]
fn resources_read_rejects_non_aghist_scheme() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"file:///etc/passwd"}}"#,
    );
    assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
}

#[test]
fn resources_read_rejects_unknown_provider() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"aghist://session/made-up/abc"}}"#,
    );
    assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
    let msg = resp["error"]["message"].as_str().unwrap();
    assert!(msg.contains("made-up"), "got: {msg}");
}

#[test]
fn resources_read_rejects_unknown_session_for_known_provider() {
    // No providers wired up, so the lookup fails on provider-not-enabled
    // before it can reach session resolution. Either way: invalid params.
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"aghist://session/claude-code/abc"}}"#,
    );
    assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
}

#[test]
fn resources_read_rejects_zero_turn() {
    let resp = run_one(
        &server(),
        r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"aghist://session/claude-code/abc/turn/0"}}"#,
    );
    assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
    let msg = resp["error"]["message"].as_str().unwrap();
    assert!(msg.contains("turn"), "got: {msg}");
}

#[test]
fn parse_uri_session_form() {
    let p = parse_aghist_uri("aghist://session/claude-code/abc-123").unwrap();
    match p {
        ParsedUri::Session {
            provider,
            session_id,
        } => {
            assert_eq!(provider, Provider::ClaudeCode);
            assert_eq!(session_id, "abc-123");
        }
        ParsedUri::Turn { .. } => panic!("expected Session form"),
    }
}

#[test]
fn parse_uri_turn_form() {
    let p = parse_aghist_uri("aghist://session/codex-cli/ses_abc/turn/7").unwrap();
    match p {
        ParsedUri::Turn {
            provider,
            session_id,
            turn,
        } => {
            assert_eq!(provider, Provider::CodexCli);
            assert_eq!(session_id, "ses_abc");
            assert_eq!(turn, 7);
        }
        ParsedUri::Session { .. } => panic!("expected Turn form"),
    }
}

#[test]
fn parse_uri_session_id_with_slash_in_path_is_treated_as_session_id() {
    // No real provider emits these today, but if a session id ever contains
    // a `/`, anything before `/turn/<n>` should still parse as the id.
    let p = parse_aghist_uri("aghist://session/claude-code/foo/bar/turn/3").unwrap();
    match p {
        ParsedUri::Turn {
            session_id, turn, ..
        } => {
            assert_eq!(session_id, "foo/bar");
            assert_eq!(turn, 3);
        }
        ParsedUri::Session { .. } => panic!("expected Turn form"),
    }
}

#[test]
fn parse_uri_rejects_bad_inputs() {
    assert!(parse_aghist_uri("file:///etc/passwd").is_err());
    assert!(parse_aghist_uri("aghist://session/").is_err());
    assert!(parse_aghist_uri("aghist://session/claude-code").is_err());
    assert!(parse_aghist_uri("aghist://session/claude-code/").is_err());
    assert!(parse_aghist_uri("aghist://session/claude-code/abc/turn/").is_err());
    assert!(parse_aghist_uri("aghist://session/claude-code/abc/turn/abc").is_err());
    assert!(parse_aghist_uri("aghist://session/claude-code/abc/turn/0").is_err());
}

#[test]
fn session_uri_round_trips_through_parser() {
    let uri = session_uri(Provider::OpenCode, "session-xyz");
    assert_eq!(uri, "aghist://session/opencode/session-xyz");
    let parsed = parse_aghist_uri(&uri).unwrap();
    match parsed {
        ParsedUri::Session {
            provider,
            session_id,
        } => {
            assert_eq!(provider, Provider::OpenCode);
            assert_eq!(session_id, "session-xyz");
        }
        ParsedUri::Turn { .. } => panic!("expected Session"),
    }
}

#[test]
fn turn_uri_round_trips_through_parser() {
    let uri = turn_uri(Provider::GeminiCli, "g-1", 42);
    assert_eq!(uri, "aghist://session/gemini-cli/g-1/turn/42");
    let parsed = parse_aghist_uri(&uri).unwrap();
    match parsed {
        ParsedUri::Turn {
            provider,
            session_id,
            turn,
        } => {
            assert_eq!(provider, Provider::GeminiCli);
            assert_eq!(session_id, "g-1");
            assert_eq!(turn, 42);
        }
        ParsedUri::Session { .. } => panic!("expected Turn"),
    }
}

#[test]
fn ping_returns_empty_object() {
    let resp = run_one(&server(), r#"{"jsonrpc":"2.0","id":10,"method":"ping"}"#);
    assert!(resp["result"].is_object());
    assert!(resp["error"].is_null());
}
