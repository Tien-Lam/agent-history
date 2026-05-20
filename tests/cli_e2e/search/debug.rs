use super::super::aghist;
use super::super::common;
use super::super::common::cli;
use predicates::prelude::*;

#[test]
fn search_help_documents_debug_search_flag() {
    aghist()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--debug-search"))
        .stdout(predicate::str::contains("BM25"));
}

#[test]
fn search_debug_search_json_includes_explanation() {
    let fixture = common::fixtures::claude::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "User", "--json", "--debug-search"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    cli::assert_success(&output);
    let doc = cli::output_stdout_json(&output);
    let arr = cli::json_array(&doc, "hits");
    assert!(!arr.is_empty(), "expected at least one hit for 'User'");

    let first = &arr[0];
    assert!(
        first.get("explanation").is_some(),
        "--debug-search must include 'explanation' field, got: {first}"
    );
    let explanation = &first["explanation"];
    assert!(
        explanation["value"].is_number(),
        "explanation must have numeric 'value', got: {explanation}"
    );
    assert!(
        explanation["description"].is_string(),
        "explanation must have 'description' string, got: {explanation}"
    );
}

#[test]
fn search_without_debug_search_omits_explanation_field() {
    let fixture = common::fixtures::claude::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "User", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    cli::assert_success(&output);
    let doc = cli::output_stdout_json(&output);
    let arr = cli::json_array(&doc, "hits");
    assert!(!arr.is_empty());
    assert!(
        arr[0].get("explanation").is_none(),
        "default search must NOT include 'explanation' field"
    );
}
