use super::super::aghist;
use super::super::common;
use super::super::common::cli;
use predicates::prelude::*;

#[test]
fn search_help_documents_hybrid_weight_flag() {
    aghist()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--hybrid-weight"))
        .stdout(predicate::str::contains("RRF"));
}

#[test]
fn search_default_engine_is_lexical_in_meta() {
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
    assert_eq!(
        doc["meta"]["engine"], "lexical",
        "default search must report engine=lexical, got: {}",
        doc["meta"]
    );
}

#[test]
fn search_hybrid_weight_falls_open_to_lexical_without_embeddings() {
    let fixture = common::fixtures::claude::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "User", "--hybrid-weight", "0.5", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    cli::assert_success(&output);
    let doc = cli::output_stdout_json(&output);
    let arr = cli::json_array(&doc, "hits");
    assert!(
        !arr.is_empty(),
        "fail-open hybrid must still return lexical hits"
    );
    assert_eq!(
        doc["meta"]["engine"], "lexical",
        "missing-embeddings build must report engine=lexical (fail-open), got: {}",
        doc["meta"]
    );
}

#[test]
fn search_hybrid_weight_zero_behaves_like_lexical() {
    let fixture = common::fixtures::claude::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "User", "--hybrid-weight", "0.0", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    cli::assert_success(&output);
    let doc = cli::output_stdout_json(&output);
    assert_eq!(
        doc["meta"]["engine"], "lexical",
        "hybrid_weight=0 must not engage hybrid path"
    );
}
