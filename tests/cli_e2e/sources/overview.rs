use super::super::aghist;
use super::super::common;

#[test]
fn sources_emits_json_with_provider_rows() {
    let fixture = common::fixtures::claude::claude_single_session(3);
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args(["sources"])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let sources = parsed["sources"].as_array().expect("sources array");
    let claude_row = sources
        .iter()
        .find(|r| r["provider"] == "claude-code")
        .expect("claude-code source row");
    assert!(claude_row["session_count"].as_u64().unwrap() >= 1);
    assert!(claude_row["paths"].as_array().is_some());
    assert!(parsed["index"]["dir"].is_string());
}
#[test]
fn sources_ndjson_one_row_per_provider() {
    let fixture = common::fixtures::claude::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args(["--ndjson", "sources"])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert!(!lines.is_empty(), "expected at least one NDJSON line");
    for line in &lines {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(row["provider"].is_string());
        assert!(row["session_count"].is_number());
    }
}
#[test]
fn sources_empty_home_exits_three() {
    let dir = tempfile::tempdir().unwrap();
    // Empty home: no providers detected — Sources should exit 3 (success-but-empty).
    let output = aghist()
        .args(["sources"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn sources_reports_size_errors_for_non_directory_paths() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join(".claude"), "not a directory").unwrap();

    let assert = aghist()
        .args(["sources"])
        .env("AGHIST_HOME", home.path())
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let sources = parsed["sources"].as_array().expect("sources array");
    let claude_row = sources
        .iter()
        .find(|r| r["provider"] == "claude-code")
        .expect("claude-code source row");
    let path = claude_row["paths"][0].as_object().expect("source path");

    assert_eq!(path["bytes"], 0);
    assert!(!path["size_error"].as_str().expect("size_error").is_empty());
}
