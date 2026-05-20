use super::super::aghist;
use super::super::common;
use super::super::common::cli;

#[test]
fn search_params_invokes_query() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-search-params")
        .project("search-params-project")
        .user("uniqueneedlephrase")
        .assistant("answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();

    let body = serde_json::json!({
        "query": "uniqueneedlephrase",
        "limit": 5,
        "json": true
    })
    .to_string();

    let assert = aghist()
        .args(["search", "--params", &body])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let parsed = cli::assert_stdout_json(&assert);
    let hits = cli::json_array(&parsed, "hits");
    assert!(
        !hits.is_empty(),
        "expected at least one hit for the unique phrase"
    );
}
