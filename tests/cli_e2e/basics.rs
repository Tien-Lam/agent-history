use super::aghist;
use predicates::prelude::*;

#[test]
fn help_flag_exits_zero() {
    aghist()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Browse and search AI agent conversation history",
        ));
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
fn update_help_exits_zero() {
    aghist()
        .args(["update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Update aghist to the latest release",
        ));
}
#[test]
fn uninstall_help_exits_zero() {
    aghist()
        .args(["uninstall", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Remove aghist binary and data"));
}
