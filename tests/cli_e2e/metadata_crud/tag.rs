use super::super::aghist;
use assert_cmd::Command;

fn tag_env(metadata_db: &std::path::Path) -> Command {
    let mut cmd = aghist();
    cmd.env("AGHIST_METADATA_DB", metadata_db);
    cmd
}
#[test]
fn tag_add_then_list_round_trips_via_json() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let added = tag_env(&db)
        .args(["tag", "add", "claude-code/abc#3", "review"])
        .output()
        .unwrap();
    assert_eq!(added.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&added.stdout).unwrap().trim()).unwrap();
    let added_id = parsed["added"]["id"].as_i64().expect("added.id is i64");
    assert!(added_id >= 1);
    assert_eq!(parsed["added"]["session_ref"], "claude-code/abc#3");
    assert_eq!(parsed["added"]["tag"], "review");

    let listed = tag_env(&db)
        .args(["tag", "list", "--json"])
        .output()
        .unwrap();
    assert_eq!(listed.status.code(), Some(0));
    let listed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&listed.stdout).unwrap().trim()).unwrap();
    assert_eq!(listed["count"], 1);
    assert_eq!(listed["tags"][0]["id"], added_id);
    assert_eq!(listed["tags"][0]["tag"], "review");
}
#[test]
fn tag_list_empty_exits_three() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let out = tag_env(&db)
        .args(["tag", "list", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["count"], 0);
    assert!(parsed["tags"].as_array().unwrap().is_empty());
}
#[test]
fn tag_list_filters_by_session_turn_and_tag_value() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    for (r, t) in [
        ("claude-code/sess", "review"),
        ("claude-code/sess#2", "todo"),
        ("claude-code/sess#5", "review"),
        ("opencode/other", "review"),
    ] {
        tag_env(&db).args(["tag", "add", r, t]).assert().success();
    }

    let scoped = tag_env(&db)
        .args(["tag", "list", "claude-code/sess", "--json"])
        .output()
        .unwrap();
    assert_eq!(scoped.status.code(), Some(0));
    let scoped: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&scoped.stdout).unwrap().trim()).unwrap();
    assert_eq!(scoped["count"], 3);

    let turn_only = tag_env(&db)
        .args(["tag", "list", "claude-code/sess#5", "--json"])
        .output()
        .unwrap();
    assert_eq!(turn_only.status.code(), Some(0));
    let turn_only: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&turn_only.stdout).unwrap().trim()).unwrap();
    assert_eq!(turn_only["count"], 1);
    assert_eq!(turn_only["tags"][0]["session_ref"], "claude-code/sess#5");

    // Tag-value filter narrows across sessions.
    let reviews = tag_env(&db)
        .args(["tag", "list", "--tag", "review", "--json"])
        .output()
        .unwrap();
    assert_eq!(reviews.status.code(), Some(0));
    let reviews: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&reviews.stdout).unwrap().trim()).unwrap();
    assert_eq!(reviews["count"], 3);

    // Combined filters AND together: claude-code/sess + review = 2.
    let combined = tag_env(&db)
        .args([
            "tag",
            "list",
            "claude-code/sess",
            "--tag",
            "review",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(combined.status.code(), Some(0));
    let combined: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&combined.stdout).unwrap().trim()).unwrap();
    assert_eq!(combined["count"], 2);
}
#[test]
fn tag_add_duplicate_returns_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    tag_env(&db)
        .args(["tag", "add", "claude-code/abc", "review"])
        .assert()
        .success();
    let dup = tag_env(&db)
        .args(["tag", "add", "claude-code/abc", "review"])
        .assert()
        .code(1);
    let stderr = String::from_utf8(dup.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "tag-conflict");
}
#[test]
fn tag_remove_deletes_row_and_returns_envelope_on_missing_pair() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let added = tag_env(&db)
        .args(["tag", "add", "claude-code/abc", "review"])
        .output()
        .unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&added.stdout).unwrap().trim()).unwrap();
    let id = parsed["added"]["id"].as_i64().unwrap();

    let removed = tag_env(&db)
        .args(["tag", "remove", "claude-code/abc", "review"])
        .output()
        .unwrap();
    assert_eq!(removed.status.code(), Some(0));
    let removed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&removed.stdout).unwrap().trim()).unwrap();
    assert_eq!(removed["removed"]["id"], id);
    assert_eq!(removed["removed"]["tag"], "review");

    // Listing now returns empty / exit 3.
    let listed = tag_env(&db)
        .args(["tag", "list", "--json"])
        .output()
        .unwrap();
    assert_eq!(listed.status.code(), Some(3));

    // Removing again yields a stable envelope.
    let missing = tag_env(&db)
        .args(["tag", "remove", "claude-code/abc", "review"])
        .assert()
        .code(1);
    let stderr = String::from_utf8(missing.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "tag-not-found");
}
#[test]
fn tag_rm_alias_works() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    tag_env(&db)
        .args(["tag", "add", "claude-code/abc", "todo"])
        .assert()
        .success();
    tag_env(&db)
        .args(["tag", "rm", "claude-code/abc", "todo"])
        .assert()
        .success();
}
#[test]
fn tag_add_rejects_invalid_ref_with_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let assert = tag_env(&db)
        .args(["tag", "add", "fake-provider/abc", "review"])
        .assert()
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "invalid-ref");
}
#[test]
fn tag_add_rejects_empty_tag_with_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let assert = tag_env(&db)
        .args(["tag", "add", "claude-code/abc", "   "])
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "usage");
}

#[test]
fn tag_add_rejects_oversized_tag_with_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");
    let oversized = "x".repeat(aghist::schema_fragments::METADATA_TAG_MAX_BYTES + 1);

    let assert = tag_env(&db)
        .args(["tag", "add", "claude-code/abc", &oversized])
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "usage");
    assert!(
        env["error"]["message"]
            .as_str()
            .unwrap()
            .contains("tag must be at most"),
        "unexpected error envelope: {env:#}"
    );
}

#[test]
fn schema_subcommand_includes_tag() {
    let out = aghist().args(["schema", "--list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let arr = parsed["subcommands"].as_array().unwrap();
    assert!(arr.iter().any(|v| v == "tag"));

    let tag_schema = aghist().args(["schema", "tag"]).output().unwrap();
    assert_eq!(tag_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&tag_schema.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["command"], "tag");
    assert!(parsed["subcommands"]["add"].is_object());
    assert!(parsed["subcommands"]["list"].is_object());
    assert!(parsed["subcommands"]["remove"].is_object());
    assert!(parsed["definitions"]["Tag"].is_object());
    assert_eq!(
        parsed["subcommands"]["add"]["params"]["properties"]["tag"]["maxLength"],
        serde_json::json!(aghist::schema_fragments::METADATA_TAG_MAX_BYTES)
    );
    assert_eq!(
        parsed["subcommands"]["add"]["params"]["properties"]["reference"]["maxLength"],
        serde_json::json!(aghist::schema_fragments::REFERENCE_MAX_BYTES)
    );
    assert_eq!(
        parsed["subcommands"]["list"]["params"]["properties"]["reference"]["maxLength"],
        serde_json::json!(aghist::schema_fragments::REFERENCE_MAX_BYTES)
    );
    assert_eq!(
        parsed["subcommands"]["list"]["params"]["properties"]["tag"]["maxLength"],
        serde_json::json!(aghist::schema_fragments::METADATA_TAG_MAX_BYTES)
    );
    assert_eq!(
        parsed["definitions"]["Tag"]["properties"]["tag"]["maxLength"],
        serde_json::json!(aghist::schema_fragments::METADATA_TAG_MAX_BYTES)
    );
}
