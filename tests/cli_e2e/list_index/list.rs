use super::super::aghist;
use super::super::common;
use super::super::common::cli;

#[test]
fn list_with_no_data_exits_three_for_empty() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    cli::assert_empty(&output);
    let stdout = cli::output_stdout(&output);
    let session_rows: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("\"meta\""))
        .collect();
    assert!(
        session_rows.is_empty(),
        "empty list under NDJSON should emit zero session rows, got: {session_rows:?}"
    );
    assert!(
        stdout.contains("\"total\":0"),
        "trailing meta row must report total=0, got: {stdout:?}"
    );
}

#[test]
fn list_with_generated_claude_fixtures() {
    let fixture = common::fixtures::claude::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    cli::assert_success(&output);
    let rows = cli::output_ndjson_session_rows(&output);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["provider"], "claude-code");
    assert_eq!(rows[0]["message_count"], 4);
}

#[test]
fn list_with_data_exits_zero() {
    let fixture = common::fixtures::claude::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    aghist()
        .arg("--list")
        .env("AGHIST_HOME", home)
        .assert()
        .code(0);
}

#[test]
fn list_with_multiple_providers() {
    let claude = common::fixtures::claude::claude_single_session(2);
    let codex = common::fixtures::codex::codex_single_session(2);

    let home = common::helpers::FixtureHome::new();
    home.add_claude(&claude);
    home.add_codex(&codex);

    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", home.path())
        .output()
        .unwrap();
    cli::assert_success(&output);
    let providers: std::collections::HashSet<String> = cli::output_ndjson_session_rows(&output)
        .iter()
        .filter_map(|row| row["provider"].as_str().map(str::to_string))
        .collect();
    assert!(providers.contains("claude-code"));
    assert!(providers.contains("codex-cli"));
}

#[test]
fn list_json_emits_single_object_with_sessions_array() {
    let fixture = common::fixtures::claude::claude_single_session(3);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["--list", "--json"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    cli::assert_success(&output);
    let doc = cli::output_stdout_json(&output);
    let sessions = cli::json_array(&doc, "sessions");
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0]["id"].is_string());
    assert!(sessions[0]["provider"].is_string());
    assert!(sessions[0]["started_at"].is_string());
    assert_eq!(sessions[0]["message_count"], 3);
}

#[test]
fn list_ndjson_emits_one_session_per_line() {
    let fixture = common::fixtures::claude::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["--list", "--ndjson"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    cli::assert_success(&output);
    let parsed = cli::output_ndjson_values(&output);
    assert_eq!(parsed.len(), 2);
    let session = &parsed[0];
    assert!(session["id"].is_string());
    assert_eq!(session["message_count"], 2);
    assert!(session.get("sessions").is_none());
    let meta_row = &parsed[1];
    assert!(
        meta_row.get("meta").is_some(),
        "last NDJSON row must be the meta envelope, got: {meta_row}"
    );
    assert_eq!(meta_row["meta"]["total"], 1);
}

#[test]
fn list_json_empty_returns_three_with_empty_array() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["--list", "--json"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    cli::assert_empty(&output);
    let doc = cli::output_stdout_json(&output);
    assert_eq!(cli::json_array(&doc, "sessions").len(), 0);
}

#[test]
fn list_rejects_json_and_ndjson_together() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["--list", "--json", "--ndjson"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let envelope = cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
}
