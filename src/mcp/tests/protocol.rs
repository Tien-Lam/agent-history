use super::*;

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
fn oversized_request_line_returns_error_and_drains_line() {
    let mut input = String::new();
    input.push_str(r#"{"jsonrpc":"2.0","id":1,"method":"ping","padding":""#);
    input.push_str(&"x".repeat(super::super::server::MAX_REQUEST_LINE_BYTES + 1));
    input.push_str("\"}\n");
    input.push_str(r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#);
    input.push('\n');

    let mut output = Vec::new();
    server()
        .serve(Cursor::new(input.as_bytes()), &mut output)
        .unwrap();
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    assert_eq!(lines.len(), 2, "got: {lines:#?}");
    assert_eq!(lines[0]["error"]["code"], ERR_INVALID_REQUEST);
    assert_eq!(lines[0]["id"], Value::Null);
    assert_eq!(lines[1]["id"], 2);
    assert!(lines[1]["result"].is_object());
}

#[test]
fn ping_returns_empty_object() {
    let resp = run_one(&server(), r#"{"jsonrpc":"2.0","id":10,"method":"ping"}"#);
    assert!(resp["result"].is_object());
    assert!(resp["error"].is_null());
}
