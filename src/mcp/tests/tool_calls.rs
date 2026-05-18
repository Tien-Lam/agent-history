use super::*;

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
