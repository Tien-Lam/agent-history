use super::aghist;
use super::common;

/// Build a Claude fixture with three known sessions, all under the same
/// `AGHIST_HOME`, returning the home path and the session ids in deterministic
/// order. Each session has a single user turn so `message_count` == 1.
fn three_session_fixture() -> (common::fixtures::core::FixtureDir, std::path::PathBuf) {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("sess-alpha")
        .project("alpha-proj")
        .user("alpha body")
        .done()
        .add_session("sess-beta")
        .project("beta-proj")
        .user("beta body")
        .done()
        .add_session("sess-gamma")
        .project("gamma-proj")
        .user("gamma body")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap().to_path_buf();
    (fixture, home)
}
fn list_session_ids_json(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v.get("id").and_then(|x| x.as_str()).map(String::from))
        .collect()
}
#[test]
fn list_filters_by_starred() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");

    // Star alpha (session-level) and gamma (turn-level). Beta stays unstarred.
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

    // Both alpha and beta tagged 'review'; only alpha starred. Combined
    // filter must produce only alpha.
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
fn search_filters_by_starred() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");
    let index_dir = tempfile::tempdir().unwrap();

    aghist()
        .args(["star", "claude-code/sess-alpha"])
        .env("AGHIST_METADATA_DB", &db)
        .assert()
        .success();

    // Word "body" matches all three sessions; --starred narrows to alpha.
    let out = aghist()
        .args(["search", "body", "--json", "--starred"])
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
    let ids: std::collections::HashSet<&str> = hits
        .iter()
        .map(|h| h["session_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["sess-alpha"].into_iter().collect());
}
#[test]
fn search_metadata_filter_no_match_exits_three() {
    let (_keep, home) = three_session_fixture();
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("metadata.db");
    let index_dir = tempfile::tempdir().unwrap();

    let out = aghist()
        .args(["search", "body", "--json", "--tag", "nonexistent"])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_METADATA_DB", &db)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
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
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let line = stderr.lines().find(|l| l.starts_with('{')).unwrap();
    let env: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(env["error"]["kind"], "usage");
}

// ─── search indexes notes (ahist-q3o.6) ────────────────────────────────────
/// Note bodies must be searchable alongside session content. This is the
/// happy path: a note whose body contains a unique term that does NOT appear
/// in any session message — so the only way for it to surface is via the
/// notes-index path.
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
/// Notes and messages can both match the same query — verify both kinds
/// surface in one response and the discriminator is set per-row.
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

    // Note rows carry note_id + ref; message rows do not.
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
/// Without any notes in the sidecar (or no sidecar at all) the search command
/// must keep working — note indexing is best-effort, not load-bearing.
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

// ─── usage subcommand ──────────────────────────────────────────────────────
#[test]
fn schema_search_includes_metadata_filter_params() {
    let out = aghist().args(["schema", "search"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&out.stdout).unwrap().trim()).unwrap();
    let props = &parsed["params"]["properties"];
    assert!(
        props["note"].is_object(),
        "search schema missing 'note' param"
    );
    assert!(
        props["tag"].is_object(),
        "search schema missing 'tag' param"
    );
    assert!(
        props["starred"].is_object(),
        "search schema missing 'starred' param"
    );
    assert_eq!(props["starred"]["type"], "boolean");
}

// ─── report subcommand ────────────────────────────────────────────────────
