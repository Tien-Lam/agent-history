use super::*;

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

    let session_template = templates
        .iter()
        .find(|template| {
            template["uriTemplate"].as_str() == Some("aghist://session/{provider}/{session_id}")
        })
        .expect("session template");
    let description = session_template["description"].as_str().unwrap();
    for provider in Provider::all() {
        assert!(
            description.contains(provider.slug()),
            "session resource template description is missing provider slug {}",
            provider.slug()
        );
    }
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
