use super::super::aghist;
use super::super::common;
use super::super::common::cli;

#[test]
fn search_json_output_wraps_hits_in_meta_envelope() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("search-json-envelope")
        .project("search-contract")
        .user("SEARCH_JSON_ENVELOPE_TOKEN prompt")
        .assistant("ordinary answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args([
            "search",
            "SEARCH_JSON_ENVELOPE_TOKEN",
            "--limit",
            "5",
            "--json",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    cli::assert_success(&output);
    let doc = cli::output_stdout_json(&output);
    let hits = cli::json_array(&doc, "hits");
    assert_eq!(hits.len(), 1, "unique fixture token should produce one hit");
    assert!(doc["meta"].is_object(), "search JSON must include 'meta'");
    assert!(doc["meta"]["total"].is_number());
    assert_eq!(doc["meta"]["engine"], "lexical");

    let hit = &hits[0];
    assert_eq!(hit["kind"], "message");
    assert_eq!(hit["source"], "local");
    assert_eq!(hit["session_id"], "search-json-envelope");
    assert!(cli::json_str(hit, "ref").ends_with("#1"));
}
