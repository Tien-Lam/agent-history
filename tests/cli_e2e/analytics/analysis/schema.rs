use super::*;

#[test]
fn schema_subcommand_includes_todos_llm_shape() {
    let todos_schema = aghist().args(["schema", "todos"]).output().unwrap();
    assert_eq!(todos_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&todos_schema.stdout).unwrap().trim()).unwrap();
    let props = &parsed["params"]["properties"];
    assert_eq!(props["llm"]["type"], "boolean");
    assert!(props["llm_model"].is_object());
    let one_of = parsed["response"]["oneOf"].as_array().unwrap();
    assert_eq!(one_of.len(), 2);
    let llm_schema = one_of
        .iter()
        .find(|s| s["properties"].get("mode").is_some())
        .expect("llm-mode schema variant present");
    let item_props = &llm_schema["properties"]["todos"]["items"]["properties"];
    for field in ["ref", "source", "description", "status_inferred"] {
        assert!(
            item_props.get(field).is_some(),
            "llm response items must include {field}"
        );
    }
}

#[test]
fn schema_subcommand_includes_threads_llm_shape() {
    let threads_schema = aghist().args(["schema", "threads"]).output().unwrap();
    assert_eq!(threads_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&threads_schema.stdout).unwrap().trim()).unwrap();
    let props = &parsed["params"]["properties"];
    assert_eq!(props["llm"]["type"], "boolean");
    assert!(props["llm_model"].is_object());
    assert_eq!(props["llm_max_sessions"]["default"], 200);
    let one_of = parsed["response"]["oneOf"].as_array().unwrap();
    assert_eq!(one_of.len(), 2);
    let llm_schema = one_of
        .iter()
        .find(|s| s["properties"].get("mode").is_some())
        .expect("llm-mode schema variant present");
    let item_props = &llm_schema["properties"]["threads"]["items"]["properties"];
    for field in ["topic_summary", "member_refs", "time_span"] {
        assert!(
            item_props.get(field).is_some(),
            "llm response items must include {field}"
        );
    }
}

#[test]
fn schema_subcommand_includes_track() {
    let out = aghist().args(["schema", "--list"]).output().unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let arr = parsed["subcommands"].as_array().unwrap();
    assert!(arr.iter().any(|v| v == "track"));

    let track_schema = aghist().args(["schema", "track"]).output().unwrap();
    assert_eq!(track_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&track_schema.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["command"], "track");
    assert_eq!(parsed["params"]["properties"]["topic"]["minLength"], 1);
    let item_props = &parsed["response"]["properties"]["timeline"]["items"]["properties"];
    assert!(item_props["session_ref"].is_object());
    assert_eq!(item_props["direction"]["enum"].as_array().unwrap().len(), 4);
}

#[test]
fn decisions_schema_documents_llm_params_and_response() {
    let out = aghist().args(["schema", "decisions"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let props = &parsed["params"]["properties"];
    assert!(props["llm"].is_object(), "llm param should be in schema");
    assert_eq!(props["llm"]["type"], "boolean");
    assert!(
        props["llm_model"].is_object(),
        "llm_model param should be in schema"
    );
    let one_of = parsed["response"]["oneOf"].as_array().unwrap();
    assert_eq!(one_of.len(), 2, "response should oneOf {{heuristic, llm}}");
    let llm_schema = one_of
        .iter()
        .find(|s| s["properties"].get("mode").is_some())
        .expect("llm-mode schema variant present");
    let item_props = &llm_schema["properties"]["decisions"]["items"]["properties"];
    for field in ["summary", "rationale", "alternatives", "ref", "source"] {
        assert!(
            item_props.get(field).is_some(),
            "llm response items must include {field}"
        );
    }
}

#[test]
fn decisions_rejects_zero_limit_flag() {
    let assert = aghist()
        .args(["decisions", "--limit", "0"])
        .assert()
        .code(2);
    let envelope = common::cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("decisions limit must be at least 1"),
        "unexpected error envelope: {envelope:#}"
    );
}

#[test]
fn threads_rejects_zero_min_sessions_flag() {
    let assert = aghist()
        .args(["threads", "--min-sessions", "0"])
        .assert()
        .code(2);
    let envelope = common::cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("min sessions must be at least 1"),
        "unexpected error envelope: {envelope:#}"
    );
}

#[test]
fn track_rejects_empty_topic() {
    let assert = aghist().args(["track", ""]).assert().code(2);
    let envelope = common::cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("track <topic> must not be empty"),
        "unexpected error envelope: {envelope:#}"
    );
}
