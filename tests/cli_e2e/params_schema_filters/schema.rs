use super::super::aghist;
use std::collections::BTreeSet;

#[test]
fn schema_list_emits_subcommand_index() {
    let assert = aghist().args(["schema", "--list"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let names = parsed["subcommands"].as_array().expect("subcommands array");
    assert!(names.iter().any(|n| n == "search"));
    assert!(names.iter().any(|n| n == "schema"));
}

#[test]
fn schema_for_search_is_valid_json_schema() {
    let assert = aghist().args(["schema", "search"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        parsed["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(parsed["command"], "search");
    assert!(parsed["params"]["properties"]["query"].is_object());
    assert!(parsed["response"].is_object());
    assert!(parsed["exit_codes"]["0"].is_string());
    assert_eq!(
        parsed["params"]["properties"]["limit"]["default"],
        serde_json::json!(aghist::schema_fragments::SEARCH_LIMIT_DEFAULT)
    );
    assert!(parsed["params"]["properties"]["cursor"].is_object());
    assert!(parsed["response"]["properties"]["hits"].is_object());
    assert!(parsed["response"]["properties"]["meta"].is_object());
}

#[test]
fn schema_command_does_not_require_loadable_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("broken.toml");
    std::fs::write(&config, "not = [valid").unwrap();

    let assert = aghist()
        .args(["schema", "search"])
        .env("AGHIST_CONFIG", &config)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["command"], "search");
}

#[test]
fn schema_for_search_documents_filter_flags() {
    let assert = aghist().args(["schema", "search"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let props = &parsed["params"]["properties"];
    for name in [
        "provider",
        "since",
        "until",
        "project",
        "role",
        "has_tool_call",
    ] {
        assert!(
            props[name].is_object(),
            "search schema missing filter param: {name}"
        );
    }
    assert_eq!(props["provider"]["type"], "string");
    assert_eq!(props["since"]["format"], "date-time");
    assert_eq!(
        props["role"]["enum"],
        serde_json::json!(["user", "assistant", "tool"])
    );
    assert_eq!(props["has_tool_call"]["type"], "boolean");
}

#[test]
fn schema_for_list_documents_filter_flags() {
    let assert = aghist().args(["schema", "list"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let props = &parsed["params"]["properties"];
    assert_eq!(
        props["limit"]["default"],
        serde_json::json!(aghist::schema_fragments::LIST_LIMIT_DEFAULT)
    );
    assert!(props["cursor"].is_object());
    for name in [
        "provider",
        "since",
        "until",
        "project",
        "role",
        "has_tool_call",
    ] {
        assert!(
            props[name].is_object(),
            "list schema missing filter param: {name}"
        );
    }
}

#[test]
fn schema_all_dumps_every_subcommand() {
    let index = aghist().args(["schema", "--list"]).assert().success();
    let index_stdout = String::from_utf8(index.get_output().stdout.clone()).unwrap();
    let index: serde_json::Value = serde_json::from_str(index_stdout.trim()).unwrap();
    let listed: BTreeSet<&str> = index["subcommands"]
        .as_array()
        .expect("subcommands array")
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect();

    let all = aghist().args(["schema", "--all"]).assert().success();
    let all_stdout = String::from_utf8(all.get_output().stdout.clone()).unwrap();
    let all: serde_json::Value = serde_json::from_str(all_stdout.trim()).unwrap();
    let map = all.as_object().expect("top-level object");
    let dumped: BTreeSet<&str> = map.keys().map(String::as_str).collect();
    assert_eq!(dumped, listed);

    for name in listed {
        assert!(map.contains_key(name), "missing schema for {name}");
        assert_eq!(map[name]["$id"], format!("aghist:schema/{name}"));
    }
}

#[test]
fn schema_unknown_subcommand_exits_one_with_envelope() {
    let output = aghist().args(["schema", "nonsense"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(stderr.trim().lines().last().unwrap()).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("nonsense"));
}

#[test]
fn schema_without_args_exits_two_usage() {
    let output = aghist().arg("schema").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}
