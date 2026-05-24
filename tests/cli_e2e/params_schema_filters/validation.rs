use super::super::aghist;
use predicates::prelude::*;

#[test]
fn filters_help_lists_all_six_flags() {
    aghist()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--provider <SLUG>"))
        .stdout(predicate::str::contains("--since <RFC3339>"))
        .stdout(predicate::str::contains("--until <RFC3339>"))
        .stdout(predicate::str::contains("--project <NAME>"))
        .stdout(predicate::str::contains("--role <ROLE>"))
        .stdout(predicate::str::contains("--has-tool-call"));
}

#[test]
fn filter_provider_rejects_unknown_slug() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["--list", "--provider", "not-a-provider"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("unknown provider slug"),
        "expected provider validation error, got: {stderr}"
    );
}

#[test]
fn filter_role_rejects_unknown_value() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["--list", "--role", "robot"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("unknown role"),
        "expected role validation error, got: {stderr}"
    );
}

#[test]
fn filter_since_rejects_non_rfc3339() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["--list", "--since", "yesterday"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("RFC 3339"),
        "expected RFC 3339 validation error, got: {stderr}"
    );
}

#[test]
fn filters_reject_inverted_time_range() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args([
            "--list",
            "--since",
            "2026-01-02T00:00:00Z",
            "--until",
            "2026-01-01T00:00:00Z",
        ])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("--since must be less than or equal to --until"),
        "expected inverted time range validation error, got: {stderr}"
    );
}
