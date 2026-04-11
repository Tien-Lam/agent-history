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
fn list_with_no_data_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .assert()
        .success()
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
fn export_nonexistent_session_fails() {
    let dir = tempfile::tempdir().unwrap();
    aghist()
        .args(["export", "--format", "md", "--session", "nonexistent"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Session not found"));
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
fn invalid_export_format_fails() {
    let dir = tempfile::tempdir().unwrap();
    aghist()
        .args(["export", "--format", "xml", "--session", "any"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value 'xml'"));
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
fn reindex_flag_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    aghist()
        .arg("--reindex")
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .assert()
        .success();
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
