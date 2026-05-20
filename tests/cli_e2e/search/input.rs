use super::super::aghist;
use super::super::common::cli;
use predicates::prelude::*;

#[test]
fn search_help_exits_zero() {
    aghist()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Search indexed sessions"));
}

#[test]
fn search_requires_query_argument() {
    aghist()
        .arg("search")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "search requires a query (positional, --query-file, or --stdin)",
        ));
}

#[test]
fn search_query_file_and_stdin_are_mutually_exclusive() {
    aghist()
        .args(["search", "--query-file", "/tmp/q.txt", "--stdin"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn search_positional_and_query_file_are_mutually_exclusive() {
    aghist()
        .args(["search", "hello", "--query-file", "/tmp/q.txt"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn search_stdin_reads_query_from_standard_input() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "--stdin", "--json"])
        .env("AGHIST_HOME", dir.path())
        .write_stdin("{some braces} \"and quotes\"\n")
        .output()
        .unwrap();

    assert_ne!(
        output.status.code(),
        Some(2),
        "stdin query should be accepted; stderr: {}",
        cli::output_stderr(&output)
    );
}

#[test]
fn search_query_file_reads_query_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let qfile = dir.path().join("q.txt");
    std::fs::write(&qfile, "{a} \"b\"\n").unwrap();
    let output = aghist()
        .args(["search", "--query-file"])
        .arg(&qfile)
        .args(["--json"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_ne!(
        output.status.code(),
        Some(2),
        "query-file should be accepted; stderr: {}",
        cli::output_stderr(&output)
    );
}

#[test]
fn search_query_file_missing_path_emits_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args([
            "search",
            "--query-file",
            "/nonexistent/path/does/not/exist.txt",
            "--json",
        ])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    cli::assert_exit_code(&output, 2);
    let stderr = cli::output_stderr(&output);
    assert!(
        stderr.contains("failed to read query file"),
        "expected io-error envelope, got: {stderr}"
    );
}

#[test]
fn search_query_file_dash_reads_from_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "--query-file", "-", "--json"])
        .env("AGHIST_HOME", dir.path())
        .write_stdin("test query\n")
        .output()
        .unwrap();
    assert_ne!(
        output.status.code(),
        Some(2),
        "--query-file - should read from stdin; stderr: {}",
        cli::output_stderr(&output)
    );
}

#[test]
fn search_empty_stdin_reports_empty_query() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "--stdin", "--json"])
        .env("AGHIST_HOME", dir.path())
        .write_stdin("   \n\n")
        .output()
        .unwrap();
    cli::assert_exit_code(&output, 2);
    let stderr = cli::output_stderr(&output);
    assert!(
        stderr.contains("search query is empty"),
        "expected empty-query envelope, got: {stderr}"
    );
}
