use super::super::aghist;
use super::super::common;
use super::super::common::cli;
use predicates::prelude::*;

#[test]
fn export_nonexistent_session_emits_envelope_and_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["export", "--format", "md", "--session", "nonexistent"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(1);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "session-not-found");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("nonexistent"));
    assert!(parsed["error"]["hint"].is_string());
}
#[test]
fn export_json_valid_output() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-export-test")
        .project("export-project")
        .display("Test export")
        .user("Hello")
        .assistant("Hi there")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args([
            "export",
            "--format",
            "json",
            "--session",
            "session-export-test",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .success();

    let parsed = cli::assert_stdout_json(&output);
    assert!(parsed.get("session").is_some());
    assert!(parsed.get("messages").is_some());
}
#[test]
fn export_markdown_to_stdout() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
            "--format",
            "md",
            "--session",
            "session-file-test",
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
fn export_source_qualified_remote_session_with_source_qualified_notes() {
    let local = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-export-shared")
        .project("local-export-project")
        .user("local body")
        .done()
        .build();
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-export-shared")
        .project("remote-export-project")
        .user("remote body")
        .assistant("remote answer")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");
    let home = local.base_path.parent().unwrap();

    aghist()
        .args([
            "note",
            "add",
            "claude-code/session-export-shared#1",
            "--body",
            "local note",
        ])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();
    aghist()
        .args([
            "note",
            "add",
            "laptop:claude-code/session-export-shared#2",
            "--body",
            "remote note",
        ])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();

    let output = aghist()
        .args([
            "export",
            "--format",
            "json",
            "--session",
            "laptop:claude-code/session-export-shared",
            "--include-notes",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .env("AGHIST_METADATA_DB", &db)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed = cli::output_stdout_json(&output);
    assert_eq!(parsed["session"]["project_name"], "remote-export-project");
    assert!(parsed["messages"].to_string().contains("remote body"));
    assert!(!parsed["messages"].to_string().contains("local body"));
    let notes = parsed["notes"].as_array().expect("remote notes");
    assert_eq!(notes.len(), 1);
    assert_eq!(
        notes[0]["session_ref"],
        "laptop:claude-code/session-export-shared#2"
    );
    assert_eq!(notes[0]["body"], "remote note");
}

#[test]
fn export_include_notes_reports_corrupt_metadata_db() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-export-corrupt-notes")
        .project("export-project")
        .user("body")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");
    std::fs::write(&db, "not a sqlite database").unwrap();

    let assert = aghist()
        .args([
            "export",
            "--format",
            "json",
            "--session",
            "session-export-corrupt-notes",
            "--include-notes",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .code(1);

    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "metadata-error");
}

#[test]
fn export_ambiguous_duplicate_session_id_requires_source_qualified_ref() {
    let local = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-export-ambiguous")
        .project("local-export-project")
        .user("local body")
        .done()
        .build();
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-export-ambiguous")
        .project("remote-export-project")
        .user("remote body")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);
    let home = local.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "export",
            "--format",
            "json",
            "--session",
            "session-export-ambiguous",
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
        .contains("laptop:claude-code/session-export-ambiguous"));
}
