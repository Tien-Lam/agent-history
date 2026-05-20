use super::super::*;

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
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
    assert!(parsed["message_count"].as_u64().unwrap() >= 2);
    assert!(parsed["token_usage"]["total_tokens"].as_u64().unwrap() > 0);
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
fn project_includes_remote_source_refs_without_local_provider() {
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("remote-project")
        .project("remote-proj")
        .user("TODO: revisit remote project refs")
        .assistant("We decided to keep SQLite instead of adding a service.")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let output = aghist()
        .args(["project", "remote-proj", "--json"])
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
    assert_eq!(parsed["session_count"], 1);
    assert_eq!(parsed["decisions"][0]["source"], "laptop");
    assert_eq!(
        parsed["decisions"][0]["ref"],
        "laptop:claude-code/remote-project#2"
    );
    assert_eq!(parsed["todos"][0]["source"], "laptop");
    assert_eq!(
        parsed["todos"][0]["ref"],
        "laptop:claude-code/remote-project#1"
    );
    assert_eq!(
        parsed["threads"][0]["session_refs"][0],
        "laptop:claude-code/remote-project"
    );
}

#[test]
fn project_match_is_case_insensitive_substring() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
