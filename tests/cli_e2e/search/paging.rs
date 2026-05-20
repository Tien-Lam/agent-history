use super::super::aghist;
use super::super::common;
use super::super::common::cli;

#[test]
fn search_invalid_cursor_returns_usage_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["search", "anything", "--cursor", "garbage!!"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let envelope = cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
}

#[test]
fn search_limit_emits_cursor_and_pages_without_duplicates() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("search-page")
        .project("paging")
        .user("PAGE_TOKEN first")
        .assistant("PAGE_TOKEN second")
        .user("PAGE_TOKEN third")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let page1 = aghist()
        .args(["search", "PAGE_TOKEN", "--json", "--limit", "1"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    cli::assert_success(&page1);
    let doc1 = cli::output_stdout_json(&page1);
    assert_eq!(cli::json_array(&doc1, "hits").len(), 1);
    let cursor = doc1["meta"]["next_cursor"]
        .as_str()
        .expect("first page should advertise a cursor");

    let page2 = aghist()
        .args([
            "search",
            "PAGE_TOKEN",
            "--json",
            "--limit",
            "1",
            "--cursor",
            cursor,
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    cli::assert_success(&page2);
    let doc2 = cli::output_stdout_json(&page2);
    assert_eq!(cli::json_array(&doc2, "hits").len(), 1);

    let first = cli::json_array(&doc1, "hits")[0]["message_id"]
        .as_str()
        .unwrap();
    let second = cli::json_array(&doc2, "hits")[0]["message_id"]
        .as_str()
        .unwrap();
    assert_ne!(first, second, "cursor page repeated the same hit");
}
