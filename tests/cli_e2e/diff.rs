use super::aghist;
use super::common;

fn parse_stdout_json(assert: &assert_cmd::assert::Assert) -> serde_json::Value {
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|err| panic!("expected JSON stdout, got {stdout:?}: {err}"))
}

#[test]
fn diff_json_orders_replacements_delete_then_insert() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("diff-left")
        .project("diff-project")
        .user("shared opening")
        .assistant("left only answer")
        .user("shared closing")
        .done()
        .add_session("diff-right")
        .project("diff-project")
        .user("shared opening")
        .assistant("right only answer")
        .user("shared closing")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "diff",
            "claude-code/diff-left",
            "claude-code/diff-right",
            "--json",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let parsed = parse_stdout_json(&assert);

    assert_eq!(parsed["changed"], 2);
    assert_eq!(parsed["same"], 2);
    assert_eq!(parsed["session1"]["turns"], 3);
    assert_eq!(parsed["session2"]["turns"], 3);

    let ops: Vec<&str> = parsed["ops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|op| op["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["same", "delete", "insert", "same"]);
    assert_eq!(parsed["ops"][1]["snippet"], "left only answer");
    assert_eq!(parsed["ops"][2]["snippet"], "right only answer");
}

#[test]
fn diff_identical_sessions_exits_empty_with_json_summary() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("diff-same-a")
        .project("diff-project")
        .user("same question")
        .assistant("same answer")
        .done()
        .add_session("diff-same-b")
        .project("diff-project")
        .user("same question")
        .assistant("same answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args([
            "diff",
            "claude-code/diff-same-a",
            "claude-code/diff-same-b",
            "--json",
        ])
        .env("AGHIST_HOME", home)
        .assert()
        .code(3);
    let parsed = parse_stdout_json(&assert);

    assert_eq!(parsed["changed"], 0);
    assert_eq!(parsed["same"], 2);
    assert_eq!(parsed["ops"].as_array().unwrap().len(), 2);
}

#[test]
fn diff_invalid_ref_emits_usage_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["diff", "not-a-session-ref", "claude-code/anything"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|line| line.starts_with('{')).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
}
