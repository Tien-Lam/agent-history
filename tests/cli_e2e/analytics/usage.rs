use super::*;

#[test]
fn usage_with_no_data_exits_three_for_empty() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .arg("usage")
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
}
#[test]
fn usage_default_groups_by_model_with_priced_costs() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-usage-1")
        .project("alpha")
        .user("hi")
        .assistant("hello")
        .done()
        .add_session("session-usage-2")
        .project("alpha")
        .user("again")
        .assistant("again-back")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .arg("usage")
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["meta"]["group_by"], "model");
    let rows = parsed["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["key"], "claude-sonnet-4-20250514");
    assert_eq!(rows[0]["session_count"], 2);
    // Each fixture assistant message contributes 100 input + 50 output tokens;
    // each session has one assistant message.
    assert_eq!(rows[0]["input_tokens"], 200);
    assert_eq!(rows[0]["output_tokens"], 100);
    // claude-sonnet-4 is in the pricing table → cost is non-null.
    assert!(rows[0]["cost_usd"].is_number());
    assert!(parsed["totals"]["cost_usd"].is_number());
    assert_eq!(parsed["totals"]["session_count"], 2);
}
#[test]
fn usage_by_provider_groups_across_models() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-prov-1")
        .project("alpha")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["usage", "--by", "provider", "--json"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["meta"]["group_by"], "provider");
    let rows = parsed["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["key"], "claude-code");
}

#[test]
fn usage_includes_remote_source_cache_without_local_provider() {
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("remote-usage")
        .project("remote-proj")
        .user("hi")
        .assistant("remote answer")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let output = aghist()
        .args(["usage", "--by", "provider", "--json"])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap().trim()).unwrap();
    let rows = parsed["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["key"], "claude-code");
    assert_eq!(rows[0]["session_count"], 1);
    assert_eq!(rows[0]["input_tokens"], 100);
    assert_eq!(rows[0]["output_tokens"], 50);
    assert_eq!(parsed["totals"]["session_count"], 1);
}

#[test]
fn usage_invalid_by_value_emits_usage_envelope() {
    let assert = aghist().args(["usage", "--by", "session"]).assert().code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("session") || stderr.contains("--by"),
        "expected error mentioning bad --by value, got: {stderr}"
    );
}

#[test]
fn usage_rejects_zero_limit_flag() {
    let assert = aghist().args(["usage", "--limit", "0"]).assert().code(2);
    let envelope = common::cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("usage limit must be at least 1"),
        "unexpected error envelope: {envelope:#}"
    );
}

#[test]
fn usage_limit_truncates_rows_but_totals_cover_all() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-limit-1")
        .project("alpha")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["usage", "--by", "project", "--limit", "1", "--json"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap().trim()).unwrap();
    let rows = parsed["rows"].as_array().unwrap();
    assert!(rows.len() <= 1);
    assert_eq!(parsed["totals"]["session_count"], 1);
}
#[test]
fn schema_subcommand_includes_usage() {
    let out = aghist().args(["schema", "--list"]).output().unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let arr = parsed["subcommands"].as_array().unwrap();
    assert!(arr.iter().any(|v| v == "usage"));

    let usage_schema = aghist().args(["schema", "usage"]).output().unwrap();
    assert_eq!(usage_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&usage_schema.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["command"], "usage");
    assert_eq!(parsed["params"]["properties"]["by"]["default"], "model");
    assert_eq!(
        parsed["params"]["properties"]["limit"]["default"],
        serde_json::json!(aghist::schema_fragments::USAGE_LIMIT_DEFAULT)
    );
    assert_eq!(
        parsed["params"]["properties"]["limit"]["maximum"],
        serde_json::json!(aghist::schema_fragments::USAGE_LIMIT_MAX)
    );
    let row_props = &parsed["definitions"]["UsageRow"]["properties"];
    assert!(row_props["cost_usd"].is_object());
    assert!(row_props["total_tokens"].is_object());
}
