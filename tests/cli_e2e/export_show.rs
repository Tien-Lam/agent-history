use super::aghist;
use super::common;
use predicates::prelude::*;

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
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("nonexistent"));
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
    let local = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-export-shared")
        .project("local-export-project")
        .user("local body")
        .done()
        .build();
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
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

    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
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
fn export_ambiguous_duplicate_session_id_requires_source_qualified_ref() {
    let local = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-export-ambiguous")
        .project("local-export-project")
        .user("local body")
        .done()
        .build();
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
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
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "ambiguous-session");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("laptop:claude-code/session-export-ambiguous"));
}

#[test]
fn show_resolves_ref_md_default() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-show-test")
        .project("show-project")
        .user("first-message-payload")
        .assistant("second-message-payload")
        .user("third-message-payload")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let reference = "claude-code/session-show-test#2";
    let assert = aghist()
        .args(["show", reference])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    // Title is the ref; only turn 2 should appear (no context).
    assert!(
        stdout.contains(reference),
        "stdout missing ref header: {stdout}"
    );
    assert!(
        stdout.contains("Turn 2"),
        "stdout missing 'Turn 2': {stdout}"
    );
    assert!(
        !stdout.contains("Turn 1"),
        "should not include turn 1 without context"
    );
    assert!(stdout.contains("second-message-payload"));
}
#[test]
fn show_includes_context_window() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-show-ctx")
        .user("alpha")
        .assistant("beta")
        .user("gamma")
        .assistant("delta")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "show",
            "claude-code/session-show-ctx#3",
            "--include-context",
            "1",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    // turn 3 ± 1 → turns 2,3,4
    assert!(stdout.contains("Turn 2"));
    assert!(stdout.contains("Turn 3"));
    assert!(stdout.contains("Turn 4"));
    assert!(!stdout.contains("Turn 1"));
}
#[test]
fn show_json_format_emits_machine_readable() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-show-json")
        .project("json-proj")
        .user("alpha")
        .assistant("beta")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "show",
            "claude-code/session-show-json#1",
            "--format",
            "json",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["ref"], "claude-code/session-show-json#1");
    assert_eq!(parsed["target_turn"], 1);
    assert_eq!(parsed["session_id"], "session-show-json");
    assert_eq!(parsed["project"], "json-proj");
    let msgs = parsed["messages"].as_array().expect("messages array");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["turn"], 1);
    assert_eq!(msgs[0]["is_target"], true);
}
#[test]
fn show_invalid_ref_emits_usage_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["show", "not-a-ref"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
}
#[test]
fn show_unknown_session_emits_session_not_found() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("real-session")
        .user("a")
        .assistant("b")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args(["show", "claude-code/does-not-exist#1"])
        .env("AGHIST_HOME", home)
        .assert()
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "session-not-found");
}
#[test]
fn show_turn_out_of_range_emits_session_not_found() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-show-oor")
        .user("a")
        .assistant("b")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args(["show", "claude-code/session-show-oor#999"])
        .env("AGHIST_HOME", home)
        .assert()
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "session-not-found");
}
#[test]
fn export_turn_range_slices_messages() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-tr-test")
        .project("tr-project")
        .user("turn-1-user")
        .assistant("turn-2-assistant")
        .user("turn-3-user")
        .assistant("turn-4-assistant")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    // Slice turns 2:3 — should keep only the middle two messages.
    let output = aghist()
        .args([
            "export",
            "--format",
            "json",
            "--session",
            "session-tr-test",
            "--turn-range",
            "2:3",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let msgs = parsed["messages"].as_array().expect("messages array");
    assert_eq!(msgs.len(), 2, "expected 2 messages from --turn-range 2:3");
}
#[test]
fn export_turn_range_open_end_clamps_to_total() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-tr-clamp")
        .project("tr-clamp")
        .user("a")
        .assistant("b")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    // 1:999 should clamp to (1,2) — both messages returned.
    let output = aghist()
        .args([
            "export",
            "--format",
            "json",
            "--session",
            "session-tr-clamp",
            "--turn-range",
            "1:999",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["messages"].as_array().unwrap().len(), 2);
}
#[test]
fn export_turn_range_invalid_emits_usage_envelope() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-tr-bad")
        .project("tr-bad")
        .user("a")
        .assistant("b")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "export",
            "--format",
            "md",
            "--session",
            "session-tr-bad",
            "--turn-range",
            "5:2",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .code(1); // ErrorEnvelope without explicit EXIT_USAGE return → exit 1
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
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
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
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
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
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
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
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
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
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
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
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
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["target_turn"], 2);
}
