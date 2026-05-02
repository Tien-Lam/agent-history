mod common;

use assert_cmd::Command;
use predicates::prelude::*;

fn aghist() -> Command {
    Command::cargo_bin("aghist").unwrap()
}

#[test]
fn help_flag_exits_zero() {
    aghist()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Browse and search AI agent conversation history"));
}

#[test]
fn version_flag_exits_zero() {
    aghist()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("aghist"));
}

#[test]
fn list_with_no_data_exits_three_for_empty() {
    let dir = tempfile::tempdir().unwrap();
    aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(3)
        .stdout(predicate::str::contains("Total: 0 sessions"));
}

#[test]
fn list_with_generated_claude_fixtures() {
    let fixture = common::fixtures::claude_single_session(4);
    // base_path is {tmpdir}/.claude, AGHIST_HOME should be the parent
    let home = fixture.base_path.parent().unwrap();
    aghist()
        .arg("--list")
        .env("AGHIST_HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude Code: 1 sessions"))
        .stdout(predicate::str::contains("Total: 1 sessions"));
}

#[test]
fn export_nonexistent_session_emits_envelope_and_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["export", "--format", "md", "--session", "nonexistent"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "session-not-found");
    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("nonexistent")
    );
    assert!(parsed["error"]["hint"].is_string());
}

#[test]
fn export_json_valid_output() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-export-test")
        .project("export-project")
        .display("Test export")
        .user("Hello")
        .assistant("Hi there")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["export", "--format", "json", "--session", "session-export-test"])
        .env("AGHIST_HOME", home)
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed.get("session").is_some());
    assert!(parsed.get("messages").is_some());
}

#[test]
fn export_markdown_to_stdout() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-md-test")
        .project("md-project")
        .user("Question")
        .assistant("Answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    aghist()
        .args(["export", "--format", "md", "--session", "session-md-test"])
        .env("AGHIST_HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("# md-project"));
}

#[test]
fn export_to_file() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-file-test")
        .project("file-project")
        .user("Question")
        .assistant("Answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let output_file = output_dir.path().join("export.md");

    aghist()
        .args([
            "export",
            "--format", "md",
            "--session", "session-file-test",
            "--output",
        ])
        .arg(&output_file)
        .env("AGHIST_HOME", home)
        .assert()
        .success();

    let content = std::fs::read_to_string(&output_file).unwrap();
    assert!(content.contains("# file-project"));
}

#[test]
fn invalid_export_format_emits_usage_envelope_and_exits_two() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["export", "--format", "xml", "--session", "any"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("xml")
    );
}

#[test]
fn unknown_subcommand_exits_two_with_usage_envelope() {
    let assert = aghist().arg("totally-unknown").assert().code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
}

#[test]
fn list_with_data_exits_zero() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    aghist()
        .arg("--list")
        .env("AGHIST_HOME", home)
        .assert()
        .code(0);
}

#[test]
fn list_with_multiple_providers() {
    let claude = common::fixtures::claude_single_session(2);
    let codex = common::fixtures::codex_single_session(2);

    // Build a unified home dir with Claude and Codex fixtures
    let home_dir = tempfile::tempdir().unwrap();
    common::helpers::copy_dir_recursive(
        &claude.base_path,
        &home_dir.path().join(".claude"),
    );
    let codex_sessions = home_dir.path().join(".codex").join("sessions");
    common::helpers::copy_dir_recursive(&codex.base_path, &codex_sessions);

    aghist()
        .arg("--list")
        .env("AGHIST_HOME", home_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude Code"))
        .stdout(predicate::str::contains("Codex CLI"));
}

#[test]
fn reindex_flag_clears_index() {
    // --reindex paired with --list against an empty home: the reindex side
    // should succeed (clear the index), and --list reports the empty exit
    // code 3 — the test asserts both signals.
    let dir = tempfile::tempdir().unwrap();
    aghist()
        .arg("--reindex")
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Search index cleared"));
}

#[test]
fn index_no_data_emits_zero_counts_json() {
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .arg("index")
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("expected JSON on stdout, got {stdout:?}: {e}"));
    assert_eq!(parsed["added"], 0);
    assert_eq!(parsed["updated"], 0);
    assert_eq!(parsed["unchanged"], 0);
    assert_eq!(parsed["sessions_total"], 0);
    assert_eq!(parsed["messages_indexed"], 0);
}

#[test]
fn index_idempotent_second_run_reports_unchanged() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-index-test")
        .project("idx-project")
        .user("Hello")
        .assistant("World")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    // First run: should add the session
    let first = aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(first.get_output().stdout.clone()).unwrap();
    let first_json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(first_json["added"], 1, "first run should classify session as 'added'");
    assert_eq!(first_json["updated"], 0);
    assert_eq!(first_json["unchanged"], 0);
    assert!(first_json["messages_indexed"].as_u64().unwrap() >= 1);

    // Second run: same fixture, should be unchanged
    let second = aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(second.get_output().stdout.clone()).unwrap();
    let second_json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(second_json["added"], 0);
    assert_eq!(second_json["updated"], 0);
    assert_eq!(second_json["unchanged"], 1, "second run should report 1 unchanged");
    assert_eq!(second_json["messages_indexed"], 0);
}

#[test]
fn index_provider_filter_restricts_scope() {
    // Build a home with both Claude and Codex sessions; --provider claude-code should
    // index only the Claude one.
    let claude = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("claude-only")
        .project("c-proj")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let codex = common::fixtures::codex_single_session(2);

    let home_dir = tempfile::tempdir().unwrap();
    common::helpers::copy_dir_recursive(
        &claude.base_path,
        &home_dir.path().join(".claude"),
    );
    common::helpers::copy_dir_recursive(
        &codex.base_path,
        &home_dir.path().join(".codex").join("sessions"),
    );
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["index", "--provider", "claude-code"])
        .env("AGHIST_HOME", home_dir.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["providers"], serde_json::json!(["claude-code"]));
    assert_eq!(parsed["sessions_total"], 1, "only Claude session is in scope");
    assert_eq!(parsed["added"], 1);
}

#[test]
fn index_unknown_provider_emits_usage_envelope_and_exits_two() {
    let home = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["index", "--provider", "bogus"])
        .env("AGHIST_HOME", home.path())
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("bogus")
    );
}

#[test]
fn update_help_exits_zero() {
    aghist()
        .args(["update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Update aghist to the latest release"));
}

#[test]
fn uninstall_help_exits_zero() {
    aghist()
        .args(["uninstall", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Remove aghist binary and data"));
}
