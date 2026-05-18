use super::super::aghist;
use super::super::common;
use super::super::common::cli;
use predicates::prelude::*;

#[test]
fn invalid_export_format_emits_usage_envelope_and_exits_two() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["export", "--format", "xml", "--session", "any"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(parsed["error"]["message"].as_str().unwrap().contains("xml"));
}
#[test]
fn export_params_replaces_individual_flags() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-params-test")
        .project("params-project")
        .user("Q")
        .assistant("A")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let body = serde_json::json!({
        "format": "md",
        "session": "session-params-test"
    })
    .to_string();

    aghist()
        .args(["export", "--params", &body])
        .env("AGHIST_HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("# params-project"));
}
#[test]
fn export_params_conflicts_with_format_flag() {
    let dir = tempfile::tempdir().unwrap();
    let body = serde_json::json!({"format": "md", "session": "x"}).to_string();

    let assert = aghist()
        .args(["export", "--format", "md", "--params", &body])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
}
#[test]
fn export_params_invalid_json_emits_usage_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["export", "--params", "{not valid"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(1); // ErrorEnvelope without explicit EXIT_USAGE return → exit 1
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not valid JSON"));
}
#[test]
fn export_params_unknown_field_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let body = serde_json::json!({
        "format": "md",
        "session": "x",
        "bogus_field": true
    })
    .to_string();
    let assert = aghist()
        .args(["export", "--params", &body])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(1);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
}
#[test]
fn export_params_invalid_format_value_emits_usage() {
    let dir = tempfile::tempdir().unwrap();
    let body = serde_json::json!({"format": "xml", "session": "x"}).to_string();
    let assert = aghist()
        .args(["export", "--params", &body])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(1);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("format"));
}
#[test]
fn export_params_with_turn_range_slices_output() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-params-range")
        .project("range-project")
        .user("first")
        .assistant("second")
        .user("third")
        .assistant("fourth")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let body = serde_json::json!({
        "format": "md",
        "session": "session-params-range",
        "turn_range": "2:3"
    })
    .to_string();

    let assert = aghist()
        .args(["export", "--params", &body])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = cli::assert_stdout(&assert);
    assert!(stdout.contains("second"));
    assert!(stdout.contains("third"));
    assert!(!stdout.contains("first"));
    assert!(!stdout.contains("fourth"));
}
#[test]
fn show_params_replaces_positional_ref() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-show-params")
        .project("show-params-project")
        .user("alpha-payload")
        .assistant("beta-payload")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let body = serde_json::json!({
        "reference": "claude-code/session-show-params#2",
        "format": "json"
    })
    .to_string();

    let assert = aghist()
        .args(["show", "--params", &body])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let parsed = cli::assert_stdout_json(&assert);
    assert_eq!(parsed["target_turn"], 2);
}
