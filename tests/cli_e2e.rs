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
    assert!(
        stdout.lines().filter(|l| !l.is_empty()).count() == 0,
        "empty list under NDJSON should emit zero lines, got: {stdout:?}"
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
        .map(|l| serde_json::from_str(l).expect("each NDJSON line must parse"))
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
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["provider"]
                .as_str()
                .unwrap()
                .to_string()
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
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 1);
    let row: serde_json::Value =
        serde_json::from_str(lines[0]).expect("each NDJSON line must be valid JSON");
    assert!(row["id"].is_string());
    assert_eq!(row["message_count"], 2);
    // NDJSON rows must NOT be wrapped in a `sessions` envelope.
    assert!(row.get("sessions").is_none());
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
