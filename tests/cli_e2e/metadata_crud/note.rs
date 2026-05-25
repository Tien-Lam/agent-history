use super::super::aghist;
use assert_cmd::Command;

fn note_env(metadata_db: &std::path::Path) -> Command {
    let mut cmd = aghist();
    cmd.env("AGHIST_METADATA_DB", metadata_db);
    // Notes don't read provider data, but main resolves AGHIST_HOME before
    // dispatch, so isolate it to keep the test hermetic.
    cmd
}
#[test]
fn note_add_then_list_round_trips_via_json() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let added = note_env(&db)
        .args(["note", "add", "claude-code/abc#3", "--body", "look at this"])
        .output()
        .unwrap();
    assert_eq!(added.status.code(), Some(0));
    let stdout = String::from_utf8(added.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let added_id = parsed["added"]["id"].as_i64().expect("added.id is i64");
    assert!(added_id >= 1);
    assert_eq!(parsed["added"]["session_ref"], "claude-code/abc#3");
    assert_eq!(parsed["added"]["body"], "look at this");

    let listed = note_env(&db)
        .args(["note", "list", "--json"])
        .output()
        .unwrap();
    assert_eq!(listed.status.code(), Some(0));
    let listed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&listed.stdout).unwrap().trim()).unwrap();
    assert_eq!(listed["count"], 1);
    assert_eq!(listed["notes"][0]["id"], added_id);
    assert_eq!(listed["notes"][0]["session_ref"], "claude-code/abc#3");
}
#[test]
fn note_list_empty_exits_three() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let out = note_env(&db)
        .args(["note", "list", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["count"], 0);
    assert!(parsed["notes"].as_array().unwrap().is_empty());
}
#[test]
fn note_list_session_filter_includes_session_and_turn_notes() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    for (r, body) in [
        ("claude-code/sess", "session-level"),
        ("claude-code/sess#2", "turn 2"),
        ("claude-code/sess#5", "turn 5"),
        ("opencode/other", "different session"),
    ] {
        note_env(&db)
            .args(["note", "add", r, "--body", body])
            .assert()
            .success();
    }

    let scoped = note_env(&db)
        .args(["note", "list", "claude-code/sess", "--json"])
        .output()
        .unwrap();
    assert_eq!(scoped.status.code(), Some(0));
    let scoped: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&scoped.stdout).unwrap().trim()).unwrap();
    assert_eq!(scoped["count"], 3);

    let turn_only = note_env(&db)
        .args(["note", "list", "claude-code/sess#5", "--json"])
        .output()
        .unwrap();
    assert_eq!(turn_only.status.code(), Some(0));
    let turn_only: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&turn_only.stdout).unwrap().trim()).unwrap();
    assert_eq!(turn_only["count"], 1);
    assert_eq!(turn_only["notes"][0]["session_ref"], "claude-code/sess#5");
}
#[test]
fn note_edit_replaces_body_and_bumps_updated_at() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let added = note_env(&db)
        .args(["note", "add", "claude-code/abc", "--body", "v1"])
        .output()
        .unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&added.stdout).unwrap().trim()).unwrap();
    let id = parsed["added"]["id"].as_i64().unwrap();
    let original_updated = parsed["added"]["updated_at"].as_str().unwrap().to_string();

    // Tiny gap so the timestamp can move at sub-second resolution.
    std::thread::sleep(std::time::Duration::from_millis(20));

    let edited = note_env(&db)
        .args(["note", "edit", &id.to_string(), "--body", "v2"])
        .output()
        .unwrap();
    assert_eq!(edited.status.code(), Some(0));
    let edited: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&edited.stdout).unwrap().trim()).unwrap();
    assert_eq!(edited["updated"]["id"], id);
    assert_eq!(edited["updated"]["body"], "v2");
    assert!(
        edited["updated"]["updated_at"].as_str().unwrap() >= original_updated.as_str(),
        "updated_at should advance"
    );
}
#[test]
fn note_remove_deletes_row_and_returns_envelope_on_missing_id() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let added = note_env(&db)
        .args(["note", "add", "claude-code/abc", "--body", "ephemeral"])
        .output()
        .unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&added.stdout).unwrap().trim()).unwrap();
    let id = parsed["added"]["id"].as_i64().unwrap();

    let removed = note_env(&db)
        .args(["note", "remove", &id.to_string()])
        .output()
        .unwrap();
    assert_eq!(removed.status.code(), Some(0));
    let removed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&removed.stdout).unwrap().trim()).unwrap();
    assert_eq!(removed["removed"]["id"], id);

    // Listing now returns empty / exit 3.
    let listed = note_env(&db)
        .args(["note", "list", "--json"])
        .output()
        .unwrap();
    assert_eq!(listed.status.code(), Some(3));

    // Removing again raises a stable envelope on stderr.
    let missing = note_env(&db)
        .args(["note", "remove", &id.to_string()])
        .assert()
        .code(1);
    let stderr = String::from_utf8(missing.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "note-not-found");
}

#[test]
fn note_edit_and_remove_reject_non_positive_ids() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    for args in [
        &["note", "edit", "0", "--body", "x"][..],
        &["note", "remove", "-1"][..],
    ] {
        let assert = note_env(&db).args(args).assert().code(2);
        let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
        let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
        let env: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(env["error"]["kind"], "usage");
        assert!(
            env["error"]["message"]
                .as_str()
                .unwrap()
                .contains("note id must be at least 1"),
            "unexpected error envelope: {env:#}"
        );
    }
}

#[test]
fn note_add_rejects_invalid_ref_with_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let assert = note_env(&db)
        .args(["note", "add", "fake-provider/abc", "--body", "x"])
        .assert()
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "invalid-ref");
}

#[test]
fn note_add_rejects_oversized_body_with_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");
    let body_file = dir.path().join("oversized-note.txt");
    let oversized = "x".repeat(aghist::schema_fragments::METADATA_NOTE_BODY_MAX_BYTES + 1);
    std::fs::write(&body_file, oversized).unwrap();

    let assert = note_env(&db)
        .args(["note", "add", "claude-code/abc", "--body-file"])
        .arg(&body_file)
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
            .contains("note body exceeds"),
        "unexpected error envelope: {env:#}"
    );
}

#[test]
fn note_add_reads_body_from_stdin() {
    use std::io::Write as _;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("metadata.db");

    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_aghist"))
        .args(["note", "add", "claude-code/abc", "--stdin"])
        .env("AGHIST_METADATA_DB", &db)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"piped body\nsecond line\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["added"]["body"], "piped body\nsecond line");
}
#[test]
fn schema_subcommand_includes_note() {
    let out = aghist().args(["schema", "--list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let arr = parsed["subcommands"].as_array().unwrap();
    assert!(arr.iter().any(|v| v == "note"));

    let note_schema = aghist().args(["schema", "note"]).output().unwrap();
    assert_eq!(note_schema.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&note_schema.stdout).unwrap().trim()).unwrap();
    assert_eq!(parsed["command"], "note");
    assert!(parsed["subcommands"]["add"].is_object());
    assert!(parsed["subcommands"]["list"].is_object());
    assert!(parsed["subcommands"]["edit"].is_object());
    assert!(parsed["subcommands"]["remove"].is_object());
    assert_eq!(
        parsed["subcommands"]["add"]["params"]["properties"]["body"]["maxLength"],
        serde_json::json!(aghist::schema_fragments::METADATA_NOTE_BODY_MAX_BYTES)
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
        parsed["subcommands"]["edit"]["params"]["properties"]["body"]["maxLength"],
        serde_json::json!(aghist::schema_fragments::METADATA_NOTE_BODY_MAX_BYTES)
    );
    assert_eq!(
        parsed["definitions"]["Note"]["properties"]["body"]["maxLength"],
        serde_json::json!(aghist::schema_fragments::METADATA_NOTE_BODY_MAX_BYTES)
    );
}
