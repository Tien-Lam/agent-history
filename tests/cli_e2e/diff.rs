use super::aghist;
use super::common;

#[test]
fn diff_json_orders_replacements_delete_then_insert() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("diff-left")
        .project("diff-project")
        .user("shared opening")
        .assistant("left only answer")
        .user("shared closing")
        .done()
        .add_session("diff-right")
        .project("diff-project")
        .user("shared opening")
        .assistant("right only answer")
        .user("shared closing")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "diff",
            "claude-code/diff-left",
            "claude-code/diff-right",
            "--json",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let parsed = common::cli::assert_stdout_json(&assert);

    assert_eq!(parsed["changed"], 2);
    assert_eq!(parsed["same"], 2);
    assert_eq!(parsed["session1"]["turns"], 3);
    assert_eq!(parsed["session2"]["turns"], 3);

    let ops: Vec<&str> = parsed["ops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|op| op["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["same", "delete", "insert", "same"]);
    assert_eq!(parsed["ops"][1]["snippet"], "left only answer");
    assert_eq!(parsed["ops"][2]["snippet"], "right only answer");
}

#[test]
fn diff_identical_sessions_exits_empty_with_json_summary() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("diff-same-a")
        .project("diff-project")
        .user("same question")
        .assistant("same answer")
        .done()
        .add_session("diff-same-b")
        .project("diff-project")
        .user("same question")
        .assistant("same answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "diff",
            "claude-code/diff-same-a",
            "claude-code/diff-same-b",
            "--json",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .code(3);
    let parsed = common::cli::assert_stdout_json(&assert);

    assert_eq!(parsed["changed"], 0);
    assert_eq!(parsed["same"], 2);
    assert_eq!(parsed["ops"].as_array().unwrap().len(), 2);
}

#[test]
fn diff_accepts_source_qualified_remote_session_refs() {
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("diff-remote-left")
        .project("diff-remote-project")
        .user("shared opening")
        .assistant("left remote answer")
        .done()
        .add_session("diff-remote-right")
        .project("diff-remote-project")
        .user("shared opening")
        .assistant("right remote answer")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let assert = aghist()
        .args([
            "diff",
            "laptop:claude-code/diff-remote-left",
            "laptop:claude-code/diff-remote-right",
            "--json",
        ])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .assert()
        .success();
    let parsed = common::cli::assert_stdout_json(&assert);

    assert_eq!(
        parsed["session1"]["ref"],
        "laptop:claude-code/diff-remote-left"
    );
    assert_eq!(
        parsed["session2"]["ref"],
        "laptop:claude-code/diff-remote-right"
    );
    assert_eq!(parsed["changed"], 2);
}

#[test]
fn diff_unqualified_duplicate_ref_requires_source_prefix() {
    let local = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("diff-shared")
        .project("local-diff-project")
        .user("local body")
        .done()
        .add_session("diff-local-other")
        .project("local-diff-project")
        .user("other local body")
        .done()
        .build();
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("diff-shared")
        .project("remote-diff-project")
        .user("remote body")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);
    let home = local.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "diff",
            "claude-code/diff-shared",
            "claude-code/diff-local-other",
            "--json",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .assert()
        .code(1);
    let parsed = common::cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "ambiguous-session");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("laptop:claude-code/diff-shared"));
}

#[test]
fn diff_invalid_ref_emits_usage_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["diff", "not-a-session-ref", "claude-code/anything"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(1);
    let parsed = common::cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
}
