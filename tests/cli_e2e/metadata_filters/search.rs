use super::super::aghist;
use super::three_session_fixture;

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
