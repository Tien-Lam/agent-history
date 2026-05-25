use super::super::aghist;
use super::super::common;
use super::{list_session_ids_json, three_session_fixture};

#[test]
fn list_filters_by_starred() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");

    aghist()
        .args(["star", "claude-code/sess-alpha"])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();
    aghist()
        .args(["star", "claude-code/sess-gamma#1"])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();

    let out = aghist()
        .args(["--list", "--starred"])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_METADATA_DB", &db)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ids = list_session_ids_json(std::str::from_utf8(&out.stdout).unwrap());
    let set: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
    assert_eq!(set, ["sess-alpha", "sess-gamma"].into_iter().collect());
}

#[test]
fn list_filters_by_tag_exact_match() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");

    aghist()
        .args(["tag", "add", "claude-code/sess-alpha", "review"])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();
    aghist()
        .args(["tag", "add", "claude-code/sess-beta", "todo"])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();
    aghist()
        .args(["tag", "add", "claude-code/sess-gamma#1", "review"])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();

    let out = aghist()
        .args(["--list", "--tag", "review"])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_METADATA_DB", &db)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let ids = list_session_ids_json(std::str::from_utf8(&out.stdout).unwrap());
    let set: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
    assert_eq!(set, ["sess-alpha", "sess-gamma"].into_iter().collect());
}

#[test]
fn list_filters_by_note_substring_case_insensitive() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");

    aghist()
        .args([
            "note",
            "add",
            "claude-code/sess-alpha",
            "--body",
            "Look at THIS bug later",
        ])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();
    aghist()
        .args([
            "note",
            "add",
            "claude-code/sess-beta",
            "--body",
            "different content",
        ])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();

    let out = aghist()
        .args(["--list", "--note", "this bug"])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_METADATA_DB", &db)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let ids = list_session_ids_json(std::str::from_utf8(&out.stdout).unwrap());
    assert_eq!(ids, vec!["sess-alpha".to_string()]);
}

#[test]
fn list_filters_combine_with_intersection() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");

    for s in ["sess-alpha", "sess-beta"] {
        aghist()
            .args(["tag", "add", &format!("claude-code/{s}"), "review"])
            .env("AGHIST_METADATA_DB", &db)
            .assert()
            .success();
    }
    aghist()
        .args(["star", "claude-code/sess-alpha"])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();

    let out = aghist()
        .args(["--list", "--tag", "review", "--starred"])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_METADATA_DB", &db)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let ids = list_session_ids_json(std::str::from_utf8(&out.stdout).unwrap());
    assert_eq!(ids, vec!["sess-alpha".to_string()]);
}

#[test]
fn list_metadata_filter_no_match_exits_three() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");

    let out = aghist()
        .args(["--list", "--starred"])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_METADATA_DB", &db)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn list_source_qualified_star_filters_remote_duplicate_session_ids() {
    let local = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("sess-shared")
        .project("local-proj")
        .user("local body")
        .done()
        .build();
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("sess-shared")
        .project("remote-proj")
        .user("remote body")
        .done()
        .build();
    let home = local.base_path.parent().unwrap();
    let source = common::helpers::laptop_remote_source(&remote.base_path);
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");

    aghist()
        .args(["star", "laptop:claude-code/sess-shared"])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();

    let out = aghist()
        .args(["--list", "--json", "--starred"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .env("AGHIST_METADATA_DB", &db)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let sessions = parsed["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], "sess-shared");
    assert_eq!(sessions[0]["source"], "laptop");

    let index_dir = tempfile::tempdir().unwrap();
    let search = aghist()
        .args(["search", "body", "--json", "--starred"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .env("AGHIST_METADATA_DB", &db)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    assert_eq!(
        search.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&search.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&search.stdout).unwrap().trim()).unwrap();
    let hits = parsed["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert!(hits.iter().all(|hit| hit["session_id"] == "sess-shared"));
    assert!(hits.iter().all(|hit| hit["source"] == "laptop"));
}

#[test]
fn list_invalid_note_substring_emits_usage_envelope() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");

    let assert = aghist()
        .args(["--list", "--note", "   "])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .code(2);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "usage");
}

#[test]
fn list_oversized_metadata_filters_emit_usage_envelope() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");

    for (flag, value) in [
        (
            "--note",
            "x".repeat(aghist::schema_fragments::METADATA_NOTE_FILTER_MAX_BYTES + 1),
        ),
        (
            "--tag",
            "x".repeat(aghist::schema_fragments::METADATA_TAG_MAX_BYTES + 1),
        ),
    ] {
        let assert = aghist()
            .args(["--list", flag, &value])
            .env("AGHIST_HOME", &home)
            .env("AGHIST_METADATA_DB", &db)
            .assert()
            .code(2);
        let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
        let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
        let env: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(env["error"]["kind"], "usage");
    }
}
