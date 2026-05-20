use super::super::aghist;
use super::super::common;
use super::super::common::cli;
use predicates::prelude::*;

#[test]
fn search_watch_help_documents_flags() {
    aghist()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--watch"))
        .stdout(predicate::str::contains("--watch-interval-ms"))
        .stdout(predicate::str::contains("--watch-iterations"));
}

#[test]
fn search_watch_emits_ndjson_one_per_line_for_existing_matches() {
    let fixture = common::fixtures::claude::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let output = aghist()
        .args([
            "search",
            "User",
            "--watch",
            "--watch-interval-ms",
            "10",
            "--watch-iterations",
            "1",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();

    cli::assert_success(&output);
    let stdout = cli::output_stdout(&output);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert!(
        !lines.is_empty(),
        "expected at least one NDJSON hit, got: {stdout:?}"
    );
    for line in &lines {
        let row: serde_json::Value =
            serde_json::from_str(line).expect("each watch line must be valid JSON");
        assert!(row["session_id"].is_string());
        assert!(row["message_id"].is_string());
        assert!(row["snippet"].is_string());
        assert!(row.get("score").is_some());
        assert!(!line.trim_start().starts_with('['));
    }
}

#[test]
fn search_watch_dedups_hits_across_polls() {
    let fixture = common::fixtures::claude::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let output = aghist()
        .args([
            "search",
            "User",
            "--watch",
            "--watch-interval-ms",
            "10",
            "--watch-iterations",
            "3",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();

    cli::assert_success(&output);
    let stdout = cli::output_stdout(&output);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

    let keys: Vec<(String, String)> = lines
        .iter()
        .map(|l| {
            let row: serde_json::Value = serde_json::from_str(l).unwrap();
            (
                cli::json_str(&row, "session_id").to_string(),
                cli::json_str(&row, "message_id").to_string(),
            )
        })
        .collect();
    let unique: std::collections::HashSet<_> = keys.iter().cloned().collect();
    assert_eq!(
        keys.len(),
        unique.len(),
        "watch must emit each (session_id, message_id) at most once across polls; got {} lines / {} unique",
        keys.len(),
        unique.len()
    );
}

#[test]
fn search_watch_requires_query() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "--watch", "--watch-iterations", "1"])
        .env("AGHIST_HOME", dir.path())
        .env("AGHIST_INDEX_DIR", dir.path().join("idx"))
        .output()
        .unwrap();
    cli::assert_exit_code(&output, 2);
    let stderr = cli::output_stderr(&output);
    assert!(
        stderr.contains("search requires a query"),
        "expected usage envelope, got: {stderr}"
    );
}
