use super::super::aghist;
use super::super::common;
use super::super::common::cli;

#[test]
fn export_turn_range_slices_messages() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
    let parsed = cli::assert_stdout_json(&output);
    let msgs = parsed["messages"].as_array().expect("messages array");
    assert_eq!(msgs.len(), 2, "expected 2 messages from --turn-range 2:3");
}
#[test]
fn export_turn_range_open_end_clamps_to_total() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
    let parsed = cli::assert_stdout_json(&output);
    assert_eq!(parsed["messages"].as_array().unwrap().len(), 2);
}
#[test]
fn export_turn_range_invalid_emits_usage_envelope() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
        .code(2);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
}

#[test]
fn export_turn_range_oversized_emits_usage_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let oversized = "1".repeat(aghist::schema_fragments::EXPORT_TURN_RANGE_MAX_BYTES + 1);

    let assert = aghist()
        .args([
            "export",
            "--format",
            "md",
            "--session",
            "session-tr-bad",
            "--turn-range",
            &oversized,
        ])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("turn range"));
}
