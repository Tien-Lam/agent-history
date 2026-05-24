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

#[test]
fn search_params_rejects_schema_range_violations() {
    let limit_body = serde_json::json!({
        "query": "anything",
        "limit": 0,
        "json": true
    })
    .to_string();
    let limit_output = aghist()
        .args(["search", "--params", &limit_body])
        .output()
        .unwrap();
    cli::assert_exit_code(&limit_output, 2);
    let limit_error = cli::output_stderr_error(&limit_output);
    assert_eq!(limit_error["error"]["kind"], "usage");
    assert!(limit_error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("search limit must be at least 1"));

    let hybrid_body = serde_json::json!({
        "query": "anything",
        "hybrid_weight": 2.0,
        "json": true
    })
    .to_string();
    let hybrid_output = aghist()
        .args(["search", "--params", &hybrid_body])
        .output()
        .unwrap();
    cli::assert_exit_code(&hybrid_output, 2);
    let hybrid_error = cli::output_stderr_error(&hybrid_output);
    assert_eq!(hybrid_error["error"]["kind"], "usage");
    assert!(hybrid_error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("hybrid_weight"));
}

#[test]
fn search_rejects_zero_limit_flag() {
    let output = aghist()
        .args(["search", "anything", "--limit", "0"])
        .output()
        .unwrap();

    cli::assert_exit_code(&output, 2);
    assert!(
        cli::output_stderr(&output).contains("search limit must be at least 1"),
        "stderr: {}",
        cli::output_stderr(&output)
    );
}
