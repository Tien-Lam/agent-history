use super::super::*;

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
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
fn report_includes_remote_source_refs_without_local_provider() {
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("remote-report")
        .project("remote-report-proj")
        .user("TODO: revisit remote report refs")
        .assistant("We decided to keep SQLite instead of adding a service.")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let output = aghist()
        .args([
            "report",
            "--since",
            FIXTURE_SINCE,
            "--until",
            FIXTURE_UNTIL,
            "--json",
        ])
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
        "laptop:claude-code/remote-report#2"
    );
    assert_eq!(parsed["todos"][0]["source"], "laptop");
    assert_eq!(
        parsed["todos"][0]["ref"],
        "laptop:claude-code/remote-report#1"
    );
    assert_eq!(
        parsed["threads"][0]["session_refs"][0],
        "laptop:claude-code/remote-report"
    );
}

#[test]
fn report_top_projects_limit_truncates_but_meta_keeps_total() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
