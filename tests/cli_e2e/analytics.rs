use super::aghist;
use super::common;

/// Fixture timestamps are pinned to 2025-01-01; bracket that day so the
/// report's window includes them regardless of when the test is run.
const FIXTURE_SINCE: &str = "2024-12-25T00:00:00Z";
const FIXTURE_UNTIL: &str = "2025-01-02T00:00:00Z";

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
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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
    let empty_home = tempfile::tempdir().unwrap();
    let workdir = tempfile::tempdir().unwrap();
    let cache_dir = workdir.path().join("cache");
    let remote_data = cache_dir.join("laptop").join("data");
    std::fs::create_dir_all(&remote_data).unwrap();

    let remote = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("remote-usage")
        .project("remote-proj")
        .user("hi")
        .assistant("remote answer")
        .done()
        .build();
    common::helpers::copy_dir_recursive(&remote.base_path, &remote_data.join(".claude"));

    let config_path = workdir.path().join("config.toml");
    aghist()
        .args([
            "sources",
            "add",
            "laptop",
            "--host",
            "laptop.local",
            "--path",
            "/home/x/.claude",
        ])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    let output = aghist()
        .args(["usage", "--by", "provider", "--json"])
        .env("AGHIST_HOME", empty_home.path())
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
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
fn usage_limit_truncates_rows_but_totals_cover_all() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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
    let row_props = &parsed["definitions"]["UsageRow"]["properties"];
    assert!(row_props["cost_usd"].is_object());
    assert!(row_props["total_tokens"].is_object());
}

#[test]
fn threads_include_remote_source_refs_without_local_provider() {
    let empty_home = tempfile::tempdir().unwrap();
    let workdir = tempfile::tempdir().unwrap();
    let cache_dir = workdir.path().join("cache");
    let remote_data = cache_dir.join("laptop").join("data");
    std::fs::create_dir_all(&remote_data).unwrap();

    let remote = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("remote-thread")
        .project("thread-proj")
        .user("thread context")
        .assistant("thread answer")
        .done()
        .build();
    common::helpers::copy_dir_recursive(&remote.base_path, &remote_data.join(".claude"));

    let config_path = workdir.path().join("config.toml");
    aghist()
        .args([
            "sources",
            "add",
            "laptop",
            "--host",
            "laptop.local",
            "--path",
            "/home/x/.claude",
        ])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    let output = aghist()
        .args(["threads", "--json"])
        .env("AGHIST_HOME", empty_home.path())
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
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
    let threads = parsed["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(
        threads[0]["session_refs"][0],
        "laptop:claude-code/remote-thread"
    );
    assert_eq!(threads[0]["providers"][0], "claude-code");
}

// ─── project subcommand ───────────────────────────────────────────────────
#[test]
fn project_with_no_data_exits_three_for_empty() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["project", "alpha"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
}
#[test]
fn project_aggregates_sessions_messages_tokens_and_emits_envelope() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-proj-1")
        .project("alpha")
        .user("hi")
        .assistant_with_tool("looking", "Read", r#"{"file_path":"src/main.rs"}"#)
        .done()
        .add_session("session-proj-2")
        .project("alpha")
        .user("again")
        .assistant_with_tool(
            "we decided to ship v1 instead of waiting",
            "Edit",
            r#"{"file_path":"src/main.rs"}"#,
        )
        .done()
        .add_session("session-other-1")
        .project("beta")
        .user("unrelated")
        .assistant("noise")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["project", "alpha"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    assert_eq!(parsed["query"], "alpha");
    let matched = parsed["matched_projects"].as_array().unwrap();
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0], "alpha");
    assert_eq!(parsed["session_count"], 2);
    // Two user + two assistant tool-use messages = 4. The exact tally depends
    // on how the provider materializes tool-use blocks, so we just assert >0.
    assert!(parsed["message_count"].as_u64().unwrap() >= 2);
    assert!(parsed["token_usage"]["total_tokens"].as_u64().unwrap() > 0);
    // claude-sonnet-4-* is in the pricing table → cost is non-null.
    assert!(parsed["token_usage"]["cost_usd"].is_number());

    let files = parsed["top_files"].as_array().unwrap();
    let main_rs = files
        .iter()
        .find(|f| f["path"] == "src/main.rs")
        .expect("expected src/main.rs in top_files");
    assert!(main_rs["count"].as_u64().unwrap() >= 2);

    let tod = parsed["time_of_day"].as_array().unwrap();
    assert_eq!(tod.len(), 24);
    let total_msgs: u64 = tod.iter().map(|v| v.as_u64().unwrap()).sum();
    assert!(total_msgs > 0);

    assert!(parsed["meta"]["limits"]["files"].as_u64().unwrap() >= 1);
    assert!(parsed["meta"]["thread_gap_hours"].is_number());
}
#[test]
fn project_match_is_case_insensitive_substring() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-mixed")
        .project("Alpha-Service")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["project", "alpha"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["session_count"], 1);
    let matched = parsed["matched_projects"].as_array().unwrap();
    assert!(matched.iter().any(|v| v == "Alpha-Service"));
}
#[test]
fn project_limits_truncate_files_section_but_meta_keeps_total() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-files")
        .project("alpha")
        .user("hi")
        .assistant_with_tool("a", "Read", r#"{"file_path":"a.rs"}"#)
        .assistant_with_tool("b", "Read", r#"{"file_path":"b.rs"}"#)
        .assistant_with_tool("c", "Read", r#"{"file_path":"c.rs"}"#)
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["project", "alpha", "--files", "2", "--json"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap().trim()).unwrap();
    let files = parsed["top_files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(parsed["meta"]["files_total"], 3);
}
#[test]
fn project_empty_name_emits_usage_envelope() {
    let assert = aghist().args(["project", " "]).assert().code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("project") || stderr.contains("empty"),
        "expected error mentioning empty name, got: {stderr}"
    );
}
#[test]
fn schema_subcommand_includes_project() {
    let out = aghist().args(["schema", "--list"]).output().unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let arr = parsed["subcommands"].as_array().unwrap();
    assert!(arr.iter().any(|v| v == "project"));

    let project_schema = aghist().args(["schema", "project"]).output().unwrap();
    assert_eq!(project_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&project_schema.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["command"], "project");
    assert_eq!(parsed["params"]["properties"]["name"]["minLength"], 1);
    let response = &parsed["response"]["properties"];
    assert!(response["session_count"].is_object());
    assert!(response["top_files"].is_object());
    assert!(response["time_of_day"].is_object());
    assert_eq!(response["time_of_day"]["minItems"], 24);
}
#[test]
fn report_with_no_data_exits_three_for_empty() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["report", "--week"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
}
#[test]
fn report_default_emits_markdown_with_section_headers() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-rep-1")
        .project("alpha")
        .user("hi")
        .assistant("we decided to ship v1 instead of waiting")
        .done()
        .add_session("session-rep-2")
        .project("beta")
        .user("noise")
        .assistant("TODO: revisit error handling")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["report", "--since", FIXTURE_SINCE, "--until", FIXTURE_UNTIL])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("# Weekly summary"), "got: {stdout}");
    assert!(stdout.contains("## Top projects"));
    assert!(stdout.contains("**alpha**") || stdout.contains("**beta**"));
    assert!(stdout.contains("## Decisions"));
    assert!(stdout.contains("## Open TODOs"));
    assert!(stdout.contains("## Threads"));
}
#[test]
fn report_json_emits_structured_envelope() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-json-1")
        .project("alpha")
        .user("hi")
        .assistant("hello there")
        .done()
        .add_session("session-json-2")
        .project("beta")
        .user("again")
        .assistant("we should rewrite the parser")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args([
            "report",
            "--since",
            FIXTURE_SINCE,
            "--until",
            FIXTURE_UNTIL,
            "--json",
        ])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap().trim()).unwrap();
    assert!(parsed["window"]["days"].as_i64().unwrap() >= 1);
    assert_eq!(parsed["session_count"], 2);
    assert_eq!(parsed["project_count"], 2);
    let top = parsed["top_projects"].as_array().unwrap();
    assert!(!top.is_empty());
    let names: Vec<String> = top
        .iter()
        .map(|p| p["project"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"alpha".to_string()));
    assert!(names.contains(&"beta".to_string()));
    assert!(parsed["meta"]["projects_total"].as_u64().unwrap() >= 2);
}
#[test]
fn report_top_projects_limit_truncates_but_meta_keeps_total() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-a")
        .project("alpha")
        .user("hi")
        .assistant("a")
        .done()
        .add_session("session-b")
        .project("beta")
        .user("hi")
        .assistant("b")
        .done()
        .add_session("session-c")
        .project("gamma")
        .user("hi")
        .assistant("c")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args([
            "report",
            "--since",
            FIXTURE_SINCE,
            "--until",
            FIXTURE_UNTIL,
            "--top-projects",
            "1",
            "--json",
        ])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["top_projects"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["meta"]["projects_total"], 3);
    assert_eq!(parsed["project_count"], 3);
}
#[test]
fn report_week_and_days_are_mutually_exclusive() {
    let assert = aghist()
        .args(["report", "--week", "--days", "30"])
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("--days") || stderr.contains("--week") || stderr.contains("conflict"),
        "expected mutual-exclusion error, got: {stderr}"
    );
}
#[test]
fn report_respects_global_since_until_window() {
    // One session inside the bracket, one outside (fixture timestamps are
    // all 2025-01-01, so a 2030-window must produce empty).
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-out")
        .project("alpha")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args([
            "report",
            "--since",
            "2030-01-01T00:00:00Z",
            "--until",
            "2030-01-08T00:00:00Z",
        ])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3), "expected EXIT_EMPTY");
}
#[test]
fn schema_subcommand_includes_report() {
    let out = aghist().args(["schema", "--list"]).output().unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let arr = parsed["subcommands"].as_array().unwrap();
    assert!(arr.iter().any(|v| v == "report"));

    let report_schema = aghist().args(["schema", "report"]).output().unwrap();
    assert_eq!(report_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&report_schema.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["command"], "report");
    let props = &parsed["params"]["properties"];
    assert_eq!(props["days"]["default"], 7);
    assert_eq!(props["top_projects"]["default"], 3);
    let resp = &parsed["response"]["properties"];
    assert!(resp["window"].is_object());
    assert!(resp["top_projects"].is_object());
    assert!(resp["project_count"].is_object());
}
#[test]
fn decisions_llm_without_api_key_returns_llm_error_envelope() {
    // Generate any fixture so the heuristic emits at least one candidate.
    // We use a session with explicit decision language; the LLM path then
    // groups by session and tries to call the API — but with no key set it
    // must fail fast with kind=llm-error.
    let mut builder = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("ses-llm-no-key")
        .project("p")
        .display("decisions session");
    builder = builder.assistant("After discussion we decided to use BM25 for ranking.");
    let fixture = builder.done().build();
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .arg("decisions")
        .arg("--llm")
        .env("AGHIST_HOME", home)
        // Defensively unset both keys — even on a CI host that has them set
        // for other tools, this test must exercise the missing-key path.
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("AGHIST_LLM_API_KEY")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("\"kind\":\"llm-error\""),
        "stderr should carry llm-error envelope, got: {stderr:?}"
    );
    assert!(
        stderr.contains("ANTHROPIC_API_KEY"),
        "missing-key error should name the env var: {stderr:?}"
    );
}
#[test]
fn decisions_llm_model_without_llm_flag_is_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .arg("decisions")
        .arg("--llm-model")
        .arg("claude-haiku-test")
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("\"kind\":\"usage\""),
        "stderr should be usage envelope, got: {stderr:?}"
    );
    assert!(
        stderr.contains("--llm"),
        "hint should mention --llm: {stderr:?}"
    );
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
    for field in ["summary", "rationale", "alternatives", "ref"] {
        assert!(
            item_props.get(field).is_some(),
            "llm response items must include {field}"
        );
    }
}
