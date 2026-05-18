use super::super::aghist;
use assert_cmd::Command;

fn star_env(metadata_db: &std::path::Path) -> Command {
    let mut cmd = aghist();
    cmd.env("AGHIST_METADATA_DB", metadata_db);
    cmd
}
#[test]
fn star_then_stars_round_trip_via_json() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let added = star_env(&db)
        .args(["star", "claude-code/abc#3"])
        .output()
        .unwrap();
    assert_eq!(added.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&added.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["starred"]["session_ref"], "claude-code/abc#3");
    assert!(!parsed["starred"]["starred_at"].as_str().unwrap().is_empty());

    let listed = star_env(&db).args(["stars", "--json"]).output().unwrap();
    assert_eq!(listed.status.code(), Some(0));
    let listed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&listed.stdout).unwrap().trim()).unwrap();
    assert_eq!(listed["count"], 1);
    assert_eq!(listed["stars"][0]["session_ref"], "claude-code/abc#3");
}
#[test]
fn stars_list_empty_exits_three() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let out = star_env(&db).args(["stars", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["count"], 0);
    assert!(parsed["stars"].as_array().unwrap().is_empty());
}
#[test]
fn stars_list_filters_by_session_or_turn() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    for r in [
        "claude-code/sess",
        "claude-code/sess#2",
        "claude-code/sess#5",
        "opencode/other",
    ] {
        star_env(&db).args(["star", r]).assert().success();
    }

    let scoped = star_env(&db)
        .args(["stars", "claude-code/sess", "--json"])
        .output()
        .unwrap();
    assert_eq!(scoped.status.code(), Some(0));
    let scoped: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&scoped.stdout).unwrap().trim()).unwrap();
    assert_eq!(scoped["count"], 3);

    let turn_only = star_env(&db)
        .args(["stars", "claude-code/sess#5", "--json"])
        .output()
        .unwrap();
    assert_eq!(turn_only.status.code(), Some(0));
    let turn_only: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&turn_only.stdout).unwrap().trim()).unwrap();
    assert_eq!(turn_only["count"], 1);
    assert_eq!(turn_only["stars"][0]["session_ref"], "claude-code/sess#5");
}
#[test]
fn star_duplicate_returns_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    star_env(&db)
        .args(["star", "claude-code/abc"])
        .assert()
        .success();
    let dup = star_env(&db)
        .args(["star", "claude-code/abc"])
        .assert()
        .code(1);
    let stderr = String::from_utf8(dup.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "star-conflict");
}
#[test]
fn unstar_deletes_row_and_returns_envelope_on_missing() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    star_env(&db)
        .args(["star", "claude-code/abc"])
        .assert()
        .success();

    let removed = star_env(&db)
        .args(["unstar", "claude-code/abc"])
        .output()
        .unwrap();
    assert_eq!(removed.status.code(), Some(0));
    let removed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&removed.stdout).unwrap().trim()).unwrap();
    assert_eq!(removed["unstarred"]["session_ref"], "claude-code/abc");

    let listed = star_env(&db).args(["stars", "--json"]).output().unwrap();
    assert_eq!(listed.status.code(), Some(3));

    let missing = star_env(&db)
        .args(["unstar", "claude-code/abc"])
        .assert()
        .code(1);
    let stderr = String::from_utf8(missing.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "star-not-found");
}
#[test]
fn star_rejects_invalid_ref_with_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let assert = star_env(&db)
        .args(["star", "fake-provider/abc"])
        .assert()
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "invalid-ref");
}
#[test]
fn schema_subcommand_includes_star_unstar_stars() {
    let out = aghist().args(["schema", "--list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let arr = parsed["subcommands"].as_array().unwrap();
    assert!(arr.iter().any(|v| v == "star"));
    assert!(arr.iter().any(|v| v == "unstar"));
    assert!(arr.iter().any(|v| v == "stars"));

    for name in ["star", "unstar", "stars"] {
        let out = aghist().args(["schema", name]).output().unwrap();
        assert_eq!(out.status.code(), Some(0), "schema {name} should exit 0");
        let parsed: serde_json::Value =
            serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
        assert_eq!(parsed["command"], name);
        assert!(parsed["definitions"]["Star"].is_object());
    }
}

// ─── metadata filters: --note / --tag / --starred ──────────────────────────
