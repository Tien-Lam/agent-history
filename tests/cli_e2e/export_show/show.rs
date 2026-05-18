use super::super::aghist;
use super::super::common;
use super::super::common::cli;

#[test]
fn show_resolves_ref_md_default() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-show-test")
        .project("show-project")
        .user("first-message-payload")
        .assistant("second-message-payload")
        .user("third-message-payload")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let reference = "claude-code/session-show-test#2";
    let assert = aghist()
        .args(["show", reference])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = cli::assert_stdout(&assert);
    // Title is the ref; only turn 2 should appear (no context).
    assert!(
        stdout.contains(reference),
        "stdout missing ref header: {stdout}"
    );
    assert!(
        stdout.contains("Turn 2"),
        "stdout missing 'Turn 2': {stdout}"
    );
    assert!(
        !stdout.contains("Turn 1"),
        "should not include turn 1 without context"
    );
    assert!(stdout.contains("second-message-payload"));
}
#[test]
fn show_includes_context_window() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-show-ctx")
        .user("alpha")
        .assistant("beta")
        .user("gamma")
        .assistant("delta")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "show",
            "claude-code/session-show-ctx#3",
            "--include-context",
            "1",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = cli::assert_stdout(&assert);
    // turn 3 ± 1 → turns 2,3,4
    assert!(stdout.contains("Turn 2"));
    assert!(stdout.contains("Turn 3"));
    assert!(stdout.contains("Turn 4"));
    assert!(!stdout.contains("Turn 1"));
}
#[test]
fn show_json_format_emits_machine_readable() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-show-json")
        .project("json-proj")
        .user("alpha")
        .assistant("beta")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "show",
            "claude-code/session-show-json#1",
            "--format",
            "json",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let parsed = cli::assert_stdout_json(&assert);
    assert_eq!(parsed["ref"], "claude-code/session-show-json#1");
    assert_eq!(parsed["target_turn"], 1);
    assert_eq!(parsed["session_id"], "session-show-json");
    assert_eq!(parsed["project"], "json-proj");
    let msgs = parsed["messages"].as_array().expect("messages array");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["turn"], 1);
    assert_eq!(msgs[0]["is_target"], true);
}

#[test]
fn show_source_qualified_remote_ref_preserves_source_in_output() {
    let local = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-show-shared")
        .project("local-show-project")
        .user("local body")
        .done()
        .build();
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-show-shared")
        .project("remote-show-project")
        .user("remote body")
        .assistant("remote answer")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);
    let home = local.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "show",
            "laptop:claude-code/session-show-shared#2",
            "--format",
            "json",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .assert()
        .success();
    let parsed = cli::assert_stdout_json(&assert);
    assert_eq!(parsed["ref"], "laptop:claude-code/session-show-shared#2");
    assert_eq!(parsed["project"], "remote-show-project");
    assert!(parsed["messages"].to_string().contains("remote answer"));
}

#[test]
fn show_unique_unqualified_remote_ref_resolves_across_sources() {
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-show-remote-only")
        .project("remote-show-project")
        .user("remote body")
        .assistant("remote answer")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let assert = aghist()
        .args([
            "show",
            "claude-code/session-show-remote-only#2",
            "--format",
            "json",
        ])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .assert()
        .success();
    let parsed = cli::assert_stdout_json(&assert);
    assert_eq!(
        parsed["ref"],
        "laptop:claude-code/session-show-remote-only#2"
    );
    assert_eq!(parsed["project"], "remote-show-project");
    assert!(parsed["messages"].to_string().contains("remote answer"));
}

#[test]
fn show_unqualified_duplicate_ref_requires_source_prefix() {
    let local = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-show-duplicate")
        .project("local-show-project")
        .user("local body")
        .done()
        .build();
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-show-duplicate")
        .project("remote-show-project")
        .user("remote body")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);
    let home = local.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "show",
            "claude-code/session-show-duplicate#1",
            "--format",
            "json",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .assert()
        .code(1);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "ambiguous-session");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("laptop:claude-code/session-show-duplicate"));
}

#[test]
fn show_invalid_ref_emits_usage_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["show", "not-a-ref"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(1);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
}
#[test]
fn show_unknown_session_emits_session_not_found() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("real-session")
        .user("a")
        .assistant("b")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args(["show", "claude-code/does-not-exist#1"])
        .env("AGHIST_HOME", home)
        .assert()
        .code(1);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "session-not-found");
}
#[test]
fn show_turn_out_of_range_emits_session_not_found() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-show-oor")
        .user("a")
        .assistant("b")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args(["show", "claude-code/session-show-oor#999"])
        .env("AGHIST_HOME", home)
        .assert()
        .code(1);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "session-not-found");
}
