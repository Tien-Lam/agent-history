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
fn list_with_no_data_exits_three_for_empty() {
    // Tests run under assert_cmd; stdout is piped, so --list auto-emits NDJSON.
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Empty list emits zero session rows; the trailing `{"meta": ...}` row
    // is always present so streaming consumers can detect end-of-stream.
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
    let fixture = common::fixtures::claude_single_session(4);
    // base_path is {tmpdir}/.claude, AGHIST_HOME should be the parent.
    // Under non-TTY (piped stdout), --list emits NDJSON: one row per session.
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let rows: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).expect("each NDJSON line must parse")
        })
        .filter(|v| v.get("id").is_some())
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["provider"], "claude_code");
    assert_eq!(rows[0]["message_count"], 4);
}

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
    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("nonexistent")
    );
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
fn health_returns_ok_envelope_with_writable_index() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let assert = aghist()
        .args(["health"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["ok"], true);
    let checks = parsed["checks"].as_array().expect("checks array");
    assert!(!checks.is_empty(), "expected at least one health check");

    // Find the providers-detected check — should be ok with the fixture.
    let providers_check = checks
        .iter()
        .find(|c| c["name"] == "providers-detected")
        .expect("providers-detected check missing");
    assert_eq!(providers_check["status"], "ok");

    assert!(parsed["summary"]["ok_count"].as_u64().unwrap() >= 2);
}

#[test]
fn health_warns_when_no_providers() {
    let dir = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();
    // Empty home, no providers — providers-detected check should warn.
    let assert = aghist()
        .args(["health"])
        .env("AGHIST_HOME", dir.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success(); // warns don't fail
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["ok"], true); // ok=true while no fails
    let providers_check = parsed["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "providers-detected")
        .unwrap()
        .clone();
    assert_eq!(providers_check["status"], "warn");
}

#[test]
fn sources_emits_json_with_provider_rows() {
    let fixture = common::fixtures::claude_single_session(3);
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
        .find(|r| r["provider"] == "claude_code")
        .expect("claude_code source row");
    assert!(claude_row["session_count"].as_u64().unwrap() >= 1);
    assert!(claude_row["paths"].as_array().is_some());
    assert!(parsed["index"]["dir"].is_string());
}

#[test]
fn sources_ndjson_one_row_per_provider() {
    let fixture = common::fixtures::claude_single_session(2);
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
    assert!(stdout.contains(reference), "stdout missing ref header: {stdout}");
    assert!(stdout.contains("Turn 2"), "stdout missing 'Turn 2': {stdout}");
    assert!(!stdout.contains("Turn 1"), "should not include turn 1 without context");
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
        .args(["show", "claude-code/session-show-ctx#3", "--include-context", "1"])
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
            "--format", "json",
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
            "--format", "json",
            "--session", "session-tr-test",
            "--turn-range", "2:3",
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
            "--format", "json",
            "--session", "session-tr-clamp",
            "--turn-range", "1:999",
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
            "--format", "md",
            "--session", "session-tr-bad",
            "--turn-range", "5:2",
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
    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("xml")
    );
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
fn list_with_data_exits_zero() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    aghist()
        .arg("--list")
        .env("AGHIST_HOME", home)
        .assert()
        .code(0);
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

    // Under non-TTY, --list emits NDJSON. Assert both providers appear.
    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", home_dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let providers: std::collections::HashSet<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            // Skip the trailing `{"meta": ...}` envelope row; only session
            // rows carry a `provider` field.
            serde_json::from_str::<serde_json::Value>(l)
                .ok()
                .and_then(|v| v["provider"].as_str().map(str::to_string))
        })
        .collect();
    assert!(providers.contains("claude_code"));
    assert!(providers.contains("codex_cli"));
}

#[test]
fn reindex_flag_clears_index() {
    // --reindex paired with --list against an empty home: the reindex side
    // should succeed (clear the index), and --list reports the empty exit
    // code 3 — the test asserts both signals.
    let dir = tempfile::tempdir().unwrap();
    aghist()
        .arg("--reindex")
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Search index cleared"));
}

#[test]
fn index_no_data_emits_zero_counts_json() {
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .arg("index")
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("expected JSON on stdout, got {stdout:?}: {e}"));
    assert_eq!(parsed["added"], 0);
    assert_eq!(parsed["updated"], 0);
    assert_eq!(parsed["unchanged"], 0);
    assert_eq!(parsed["sessions_total"], 0);
    assert_eq!(parsed["messages_indexed"], 0);
}

#[test]
fn index_idempotent_second_run_reports_unchanged() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-index-test")
        .project("idx-project")
        .user("Hello")
        .assistant("World")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    // First run: should add the session
    let first = aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(first.get_output().stdout.clone()).unwrap();
    let first_json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(first_json["added"], 1, "first run should classify session as 'added'");
    assert_eq!(first_json["updated"], 0);
    assert_eq!(first_json["unchanged"], 0);
    assert!(first_json["messages_indexed"].as_u64().unwrap() >= 1);

    // Second run: same fixture, should be unchanged
    let second = aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(second.get_output().stdout.clone()).unwrap();
    let second_json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(second_json["added"], 0);
    assert_eq!(second_json["updated"], 0);
    assert_eq!(second_json["unchanged"], 1, "second run should report 1 unchanged");
    assert_eq!(second_json["messages_indexed"], 0);
}

#[test]
fn index_provider_filter_restricts_scope() {
    // Build a home with both Claude and Codex sessions; --provider claude-code should
    // index only the Claude one.
    let claude = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("claude-only")
        .project("c-proj")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let codex = common::fixtures::codex_single_session(2);

    let home_dir = tempfile::tempdir().unwrap();
    common::helpers::copy_dir_recursive(
        &claude.base_path,
        &home_dir.path().join(".claude"),
    );
    common::helpers::copy_dir_recursive(
        &codex.base_path,
        &home_dir.path().join(".codex").join("sessions"),
    );
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["index", "--provider", "claude-code"])
        .env("AGHIST_HOME", home_dir.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["providers"], serde_json::json!(["claude-code"]));
    assert_eq!(parsed["sessions_total"], 1, "only Claude session is in scope");
    assert_eq!(parsed["added"], 1);
}

#[test]
fn index_unknown_provider_emits_usage_envelope_and_exits_two() {
    let home = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["index", "--provider", "bogus"])
        .env("AGHIST_HOME", home.path())
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON envelope on stderr");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("bogus")
    );
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
    // No data, so we expect EXIT_EMPTY (3) — but critically, NOT EXIT_USAGE (2)
    // and NOT a clap parse error. The query was accepted from stdin.
    assert_ne!(
        output.status.code(),
        Some(2),
        "stdin query should be accepted; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
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
        String::from_utf8_lossy(&output.stderr)
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
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
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
        String::from_utf8_lossy(&output.stderr)
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
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("search query is empty"),
        "expected empty-query envelope, got: {stderr}"
    );
}

#[test]
fn search_help_documents_debug_search_flag() {
    aghist()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--debug-search"))
        .stdout(predicate::str::contains("BM25"));
}

#[test]
fn search_debug_search_json_includes_explanation() {
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "User", "--json", "--debug-search"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let doc: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("--debug-search --json must emit valid JSON");
    let arr = doc["hits"].as_array().expect("expected JSON array of hits");
    assert!(!arr.is_empty(), "expected at least one hit for 'User'");

    let first = &arr[0];
    assert!(
        first.get("explanation").is_some(),
        "--debug-search must include 'explanation' field, got: {first}"
    );
    let explanation = &first["explanation"];
    assert!(
        explanation["value"].is_number(),
        "explanation must have numeric 'value', got: {explanation}"
    );
    assert!(
        explanation["description"].is_string(),
        "explanation must have 'description' string, got: {explanation}"
    );
}

#[test]
fn search_without_debug_search_omits_explanation_field() {
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "User", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let doc: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let arr = doc["hits"].as_array().expect("expected array");
    assert!(!arr.is_empty());
    assert!(
        arr[0].get("explanation").is_none(),
        "default search must NOT include 'explanation' field"
    );
}

#[test]
fn list_json_emits_single_object_with_sessions_array() {
    let fixture = common::fixtures::claude_single_session(3);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["--list", "--json"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let doc: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("--list --json must emit valid JSON");
    let sessions = doc["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0]["id"].is_string());
    assert!(sessions[0]["provider"].is_string());
    assert!(sessions[0]["started_at"].is_string());
    assert_eq!(sessions[0]["message_count"], 3);
}

#[test]
fn list_ndjson_emits_one_session_per_line() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["--list", "--ndjson"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("each NDJSON line must be valid JSON"))
        .collect();
    // One session row + one trailing `{"meta": ...}` envelope row.
    assert_eq!(parsed.len(), 2);
    let session = &parsed[0];
    assert!(session["id"].is_string());
    assert_eq!(session["message_count"], 2);
    // NDJSON session rows must NOT be wrapped in a `sessions` envelope.
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
    assert_eq!(output.status.code(), Some(3));
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(doc["sessions"].as_array().unwrap().len(), 0);
}

#[test]
fn list_rejects_json_and_ndjson_together() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["--list", "--json", "--ndjson"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let envelope: serde_json::Value = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .and_then(|l| serde_json::from_str(l).ok())
        .expect("expected JSON envelope on stderr");
    assert_eq!(envelope["error"]["kind"], "usage");
}

#[test]
fn list_limit_caps_returned_sessions_and_emits_next_cursor() {
    let fixture = common::fixtures::claude_multi_session(5, 2);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["--list", "--json", "--limit", "2"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let sessions = doc["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 2, "limit must cap returned rows");
    assert_eq!(doc["meta"]["total"], 5, "total reflects all matching rows");
    assert!(
        doc["meta"]["next_cursor"].is_string(),
        "next_cursor must be set when more results exist"
    );
}

#[test]
fn list_cursor_resumes_after_prior_page_and_paginates_to_completion() {
    let fixture = common::fixtures::claude_multi_session(5, 2);
    let home = fixture.base_path.parent().unwrap();

    // First page.
    let page1 = aghist()
        .args(["--list", "--json", "--limit", "2"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(page1.status.code(), Some(0));
    let doc1: serde_json::Value = serde_json::from_slice(&page1.stdout).unwrap();
    let cursor1 = doc1["meta"]["next_cursor"].as_str().unwrap().to_string();
    let ids1: Vec<String> = doc1["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();

    // Second page (resume).
    let page2 = aghist()
        .args(["--list", "--json", "--limit", "2", "--cursor", &cursor1])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(page2.status.code(), Some(0));
    let doc2: serde_json::Value = serde_json::from_slice(&page2.stdout).unwrap();
    let ids2: Vec<String> = doc2["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids2.len(), 2);
    let cursor2 = doc2["meta"]["next_cursor"].as_str().unwrap().to_string();

    // Third (final) page — has the last session and no further cursor.
    let page3 = aghist()
        .args(["--list", "--json", "--limit", "2", "--cursor", &cursor2])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(page3.status.code(), Some(0));
    let doc3: serde_json::Value = serde_json::from_slice(&page3.stdout).unwrap();
    let ids3: Vec<String> = doc3["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids3.len(), 1);
    assert!(
        doc3["meta"]["next_cursor"].is_null(),
        "final page must not advertise another cursor"
    );

    // No id appears across pages (no skips, no duplicates).
    let all_ids: Vec<&String> = ids1.iter().chain(ids2.iter()).chain(ids3.iter()).collect();
    let unique: std::collections::HashSet<&&String> = all_ids.iter().collect();
    assert_eq!(unique.len(), all_ids.len(), "pagination must not duplicate ids");
    assert_eq!(all_ids.len(), 5, "all 5 sessions must be visited");
}

#[test]
fn list_invalid_cursor_returns_usage_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["--list", "--cursor", "not-a-real-cursor!!!"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let envelope: serde_json::Value = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .and_then(|l| serde_json::from_str(l).ok())
        .expect("expected JSON envelope on stderr");
    assert_eq!(envelope["error"]["kind"], "usage");
}

#[test]
fn list_cursor_requires_list_flag() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["--cursor", "abc"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let envelope: serde_json::Value = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .and_then(|l| serde_json::from_str(l).ok())
        .expect("expected JSON envelope on stderr");
    assert_eq!(envelope["error"]["kind"], "usage");
}

#[test]
fn search_invalid_cursor_returns_usage_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["search", "anything", "--cursor", "garbage!!"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let envelope: serde_json::Value = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .and_then(|l| serde_json::from_str(l).ok())
        .expect("expected JSON envelope on stderr");
    assert_eq!(envelope["error"]["kind"], "usage");
}

#[test]
fn search_json_output_wraps_hits_in_meta_envelope() {
    let fixture = common::fixtures::claude_multi_session(2, 4);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["search", "User", "--limit", "5", "--json"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    // EXIT_OK or EXIT_EMPTY (3) — both are acceptable; we only assert the
    // envelope shape when hits exist.
    if output.status.code() == Some(0) {
        let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(doc["hits"].is_array(), "search JSON must wrap rows in 'hits'");
        assert!(doc["meta"].is_object(), "search JSON must include 'meta'");
        assert!(doc["meta"]["total"].is_number());
    }
}

#[test]
fn uninstall_help_exits_zero() {
    aghist()
        .args(["uninstall", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Remove aghist binary and data"));
}

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
    let fixture = common::fixtures::claude_single_session(4);
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

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert!(!lines.is_empty(), "expected at least one NDJSON hit, got: {stdout:?}");
    for line in &lines {
        let row: serde_json::Value =
            serde_json::from_str(line).expect("each watch line must be valid JSON");
        assert!(row["session_id"].is_string());
        assert!(row["message_id"].is_string());
        assert!(row["snippet"].is_string());
        assert!(row.get("score").is_some());
        // Watch NDJSON must NOT wrap rows in an array envelope.
        assert!(!line.trim_start().starts_with('['));
    }
}

#[test]
fn search_watch_dedups_hits_across_polls() {
    // Same fixture across 3 polls — every hit should appear exactly once.
    let fixture = common::fixtures::claude_single_session(4);
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

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

    let keys: Vec<(String, String)> = lines
        .iter()
        .map(|l| {
            let row: serde_json::Value = serde_json::from_str(l).unwrap();
            (
                row["session_id"].as_str().unwrap().to_string(),
                row["message_id"].as_str().unwrap().to_string(),
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
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("search requires a query"),
        "expected usage envelope, got: {stderr}"
    );
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
    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not valid JSON")
    );
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
    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("format")
    );
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

#[test]
fn search_params_invokes_query() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-search-params")
        .project("search-params-project")
        .user("uniqueneedlephrase")
        .assistant("answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    // Build the index first so search has something to find.
    aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();

    let body = serde_json::json!({
        "query": "uniqueneedlephrase",
        "limit": 5,
        "json": true
    })
    .to_string();

    let assert = aghist()
        .args(["search", "--params", &body])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let hits = parsed["hits"]
        .as_array()
        .expect("search JSON must wrap rows in 'hits'");
    assert!(!hits.is_empty(), "expected at least one hit for the unique phrase");
}

#[test]
fn index_params_force_flag() {
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();
    let body = serde_json::json!({"force": true}).to_string();

    aghist()
        .args(["index", "--params", &body])
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
}

#[test]
fn index_params_unknown_provider_slug_emits_usage() {
    let home = tempfile::tempdir().unwrap();
    let body = serde_json::json!({"provider": "bogus"}).to_string();
    let assert = aghist()
        .args(["index", "--params", &body])
        .env("AGHIST_HOME", home.path())
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
fn index_help_documents_accept_download_flag() {
    aghist()
        .args(["index", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--accept-download"))
        .stdout(predicate::str::contains("AllMiniLML6V2"));
}

#[test]
fn index_summary_includes_embeddings_block() {
    // Without the `embeddings` cargo feature compiled in, the summary should
    // surface that explicitly so callers (and humans) know nothing semantic
    // happened — even when --accept-download is passed.
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["index", "--accept-download"])
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("expected JSON on stdout, got {stdout:?}: {e}"));
    let block = &parsed["embeddings"];
    assert!(block.is_object(), "expected embeddings object, got {block}");
    let status = block["status"].as_str().unwrap_or("");
    // The lean default build reports "disabled"; a feature build reports
    // "awaiting-consent" or "enabled". Accept any of those — the contract is
    // that the field exists and tells the caller what happened.
    assert!(
        matches!(status, "disabled" | "awaiting-consent" | "enabled"),
        "unexpected embeddings status: {status:?}"
    );
}

#[test]
fn schema_list_emits_subcommand_index() {
    let assert = aghist().args(["schema", "--list"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let names = parsed["subcommands"].as_array().expect("subcommands array");
    assert!(names.iter().any(|n| n == "search"));
    assert!(names.iter().any(|n| n == "schema"));
}

#[test]
fn schema_for_search_is_valid_json_schema() {
    let assert = aghist().args(["schema", "search"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(
        parsed["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(parsed["command"], "search");
    assert!(parsed["params"]["properties"]["query"].is_object());
    assert!(parsed["response"].is_object());
    assert!(parsed["exit_codes"]["0"].is_string());
}

#[test]
fn schema_for_search_documents_filter_flags() {
    let assert = aghist().args(["schema", "search"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let props = &parsed["params"]["properties"];
    for name in ["provider", "since", "until", "project", "role", "has_tool_call"] {
        assert!(
            props[name].is_object(),
            "search schema missing filter param: {name}"
        );
    }
    assert_eq!(props["provider"]["type"], "string");
    assert_eq!(props["since"]["format"], "date-time");
    assert_eq!(props["role"]["enum"], serde_json::json!(["user", "assistant", "tool"]));
    assert_eq!(props["has_tool_call"]["type"], "boolean");
}

#[test]
fn schema_for_list_documents_filter_flags() {
    let assert = aghist().args(["schema", "list"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let props = &parsed["params"]["properties"];
    for name in ["provider", "since", "until", "project", "role", "has_tool_call"] {
        assert!(
            props[name].is_object(),
            "list schema missing filter param: {name}"
        );
    }
}

#[test]
fn schema_all_dumps_every_subcommand() {
    let assert = aghist().args(["schema", "--all"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let map = parsed.as_object().expect("top-level object");
    for name in ["list", "search", "show", "export", "index", "sources", "health", "mcp", "schema"] {
        assert!(map.contains_key(name), "missing schema for {name}");
        assert_eq!(map[name]["$id"], format!("aghist:schema/{name}"));
    }
}

#[test]
fn schema_unknown_subcommand_exits_one_with_envelope() {
    let output = aghist().args(["schema", "nonsense"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stderr.trim().lines().last().unwrap()).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(parsed["error"]["message"].as_str().unwrap().contains("nonsense"));
}

#[test]
fn schema_without_args_exits_two_usage() {
    let output = aghist().arg("schema").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

// ─── Filters: --provider --since --until --project --role --has-tool-call ───
//
// These flags apply to both `--list` and `search`. They're declared as global
// on the top-level `Cli` so users can place them either before or after the
// subcommand name.

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
fn list_filter_provider_drops_other_providers() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();

    let kept = aghist()
        .args(["--list", "--ndjson", "--provider", "claude-code"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(kept.status.code(), Some(0));
    let stdout = String::from_utf8(kept.stdout).unwrap();
    let session_rows: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("\"meta\""))
        .collect();
    assert!(
        session_rows.len() == 1,
        "expected one row for claude-code, got: {session_rows:?}"
    );

    let dropped = aghist()
        .args(["--list", "--ndjson", "--provider", "codex-cli"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    // No codex sessions in fixture → EXIT_EMPTY (3) with zero session rows
    // (the trailing `{"meta":...}` envelope row is still emitted).
    assert_eq!(dropped.status.code(), Some(3));
    let stdout = String::from_utf8(dropped.stdout).unwrap();
    let session_rows: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("\"meta\""))
        .collect();
    assert!(
        session_rows.is_empty(),
        "expected zero rows for codex-cli, got: {session_rows:?}"
    );
}

#[test]
fn list_filter_since_excludes_older_sessions() {
    // Default fixture session is at 2025-01-01T00:00:00Z. Picking a since
    // strictly after that should drop it.
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["--list", "--ndjson", "--since", "2025-06-01T00:00:00Z"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let session_rows: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("\"meta\""))
        .collect();
    assert!(
        session_rows.is_empty(),
        "since cutoff should drop older session, got: {session_rows:?}"
    );
}

#[test]
fn list_filter_until_includes_older_sessions() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["--list", "--ndjson", "--until", "2030-01-01T00:00:00Z"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let session_rows: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("\"meta\""))
        .collect();
    assert_eq!(session_rows.len(), 1);
}

#[test]
fn list_filter_project_substring_match_is_case_insensitive() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-proj-test")
        .project("MyCoolProject")
        .user("hi")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let hit = aghist()
        .args(["--list", "--ndjson", "--project", "coolproj"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(hit.status.code(), Some(0));

    let miss = aghist()
        .args(["--list", "--ndjson", "--project", "nope"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(miss.status.code(), Some(3));
}

#[test]
fn list_filter_role_drops_sessions_without_matching_messages() {
    // Session has only user/assistant messages; --role tool should drop it.
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-role-test")
        .project("role-test")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let kept = aghist()
        .args(["--list", "--ndjson", "--role", "user"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(kept.status.code(), Some(0));

    let dropped = aghist()
        .args(["--list", "--ndjson", "--role", "tool"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(dropped.status.code(), Some(3));
}

#[test]
fn list_filter_has_tool_call_keeps_only_sessions_with_tool_use() {
    // Session A: tool-use; Session B: text only.
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-with-tool")
        .project("tool-yes")
        .user("run a thing")
        .assistant_with_tool("running", "Bash", r#"{"command":"ls"}"#)
        .done()
        .add_session("session-no-tool")
        .project("tool-no")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let output = aghist()
        .args(["--list", "--ndjson", "--has-tool-call"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let rows: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|v| v.get("id").is_some())
        .collect();
    assert_eq!(rows.len(), 1, "expected only the tool-using session");
    assert_eq!(rows[0]["id"], "session-with-tool");
}

#[test]
fn search_filter_provider_pushes_into_index_query() {
    // Single-provider fixture; --provider matching should keep results,
    // --provider mismatching should empty them.
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let kept = aghist()
        .args(["search", "User", "--json", "--provider", "claude-code"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(kept.status.code(), Some(0));

    let index2 = tempfile::tempdir().unwrap();
    let dropped = aghist()
        .args(["search", "User", "--json", "--provider", "codex-cli"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index2.path())
        .output()
        .unwrap();
    // No codex-cli docs match → EXIT_EMPTY.
    assert_eq!(dropped.status.code(), Some(3));
}

#[test]
fn search_filter_role_restricts_hits_to_matching_role() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-search-role")
        .project("rolesearch")
        .user("apple banana")
        .assistant("cherry banana")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let user_only = aghist()
        .args(["search", "banana", "--json", "--role", "user"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(user_only.status.code(), Some(0));
    let stdout = String::from_utf8(user_only.stdout).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let rows = doc["hits"].as_array().expect("search JSON must have hits");
    assert_eq!(rows.len(), 1, "user-role filter should leave one hit");
    let snippet = rows[0]["snippet"].as_str().unwrap_or("");
    assert!(
        snippet.contains("apple"),
        "expected user message in hit, got snippet: {snippet}"
    );
}

#[test]
fn search_filter_has_tool_call_keeps_only_tool_messages() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-search-tool")
        .project("toolsearch")
        .user("search-keyword run me")
        .assistant_with_tool("ok", "Bash", r#"{"command":"search-keyword"}"#)
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "search-keyword", "--json", "--has-tool-call"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let rows = doc["hits"].as_array().expect("search JSON must have hits");
    // Both messages contain "search-keyword", but only the assistant one has
    // a tool invocation — has-tool-call should drop the user message.
    assert_eq!(rows.len(), 1, "expected only the tool-using assistant hit");
}

#[test]
fn search_filter_since_drops_old_messages() {
    // Fixture timestamps are at 2025-01-01T00:00:00–05Z. A future since cuts everything.
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let output = aghist()
        .args([
            "search",
            "User",
            "--json",
            "--since",
            "2030-01-01T00:00:00Z",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn search_filter_project_substring_match_is_case_insensitive() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-search-proj")
        .project("AwesomeProject")
        .user("findme keyword")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let hit = aghist()
        .args(["search", "findme", "--json", "--project", "awesome"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(hit.status.code(), Some(0));

    let index2 = tempfile::tempdir().unwrap();
    let miss = aghist()
        .args(["search", "findme", "--json", "--project", "nomatch"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index2.path())
        .output()
        .unwrap();
    assert_eq!(miss.status.code(), Some(3));
}
