use super::super::aghist;
use super::three_session_fixture;
use std::fs;

#[test]
fn search_returns_note_hits_with_kind_note_and_ref() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");
    let index_dir = tempfile::tempdir().unwrap();

    aghist()
        .args([
            "note",
            "add",
            "claude-code/sess-alpha#3",
            "--body",
            "investigate xylophone bug",
        ])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();

    let out = aghist()
        .args(["search", "xylophone", "--json"])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_METADATA_DB", &db)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let hits = doc["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1, "expected one note hit, got {hits:?}");
    let hit = &hits[0];
    assert_eq!(hit["kind"], "note");
    assert_eq!(hit["ref"], "claude-code/sess-alpha#3");
    assert!(hit["note_id"].as_i64().unwrap() >= 1);
    assert!(hit["snippet"].as_str().unwrap().contains("xylophone"));
}

#[test]
fn search_mixes_note_and_message_hits() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");
    let index_dir = tempfile::tempdir().unwrap();

    aghist()
        .args([
            "note",
            "add",
            "claude-code/sess-alpha",
            "--body",
            "alpha body annotation",
        ])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();

    let out = aghist()
        .args(["search", "alpha", "--json"])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_METADATA_DB", &db)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let doc: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let hits = doc["hits"].as_array().unwrap();
    let kinds: Vec<&str> = hits.iter().map(|h| h["kind"].as_str().unwrap()).collect();
    assert!(
        kinds.contains(&"note"),
        "expected at least one note hit: {hits:?}"
    );
    assert!(
        kinds.contains(&"message"),
        "expected at least one message hit: {hits:?}"
    );

    for h in hits {
        match h["kind"].as_str().unwrap() {
            "note" => {
                assert!(h["note_id"].as_i64().is_some());
                assert!(h["ref"].as_str().is_some());
            }
            "message" => {
                assert!(h.get("note_id").is_none_or(serde_json::Value::is_null));
                assert!(!h["session_id"].as_str().unwrap().is_empty());
            }
            other => panic!("unexpected kind: {other}"),
        }
    }
}

#[test]
fn search_works_when_metadata_db_is_absent() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let missing = db_dir.path().join("nonexistent.db");
    let index_dir = tempfile::tempdir().unwrap();

    let out = aghist()
        .args(["search", "alpha body", "--json"])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_METADATA_DB", &missing)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let hits = doc["hits"].as_array().unwrap();
    assert!(
        !hits.is_empty(),
        "alpha body should match a session message"
    );
    for h in hits {
        assert_eq!(h["kind"], "message");
    }
}

#[test]
fn search_warns_but_keeps_message_results_when_metadata_db_is_unreadable() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");
    let index_dir = tempfile::tempdir().unwrap();
    fs::write(&db, b"not sqlite").unwrap();

    let out = aghist()
        .args(["search", "alpha body", "--json"])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_METADATA_DB", &db)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("warning: metadata: failed to read metadata notes"),
        "expected metadata warning, got {stderr:?}"
    );
    let doc: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    assert!(
        !doc["hits"].as_array().unwrap().is_empty(),
        "metadata warning should not suppress message hits"
    );
}
