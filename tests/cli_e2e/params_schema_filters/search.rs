use super::super::aghist;
use super::super::common;

#[test]
fn search_filter_provider_pushes_into_index_query() {
    let fixture = common::fixtures::claude::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let kept = aghist()
        .args(["search", "User", "--json", "--provider", "claude-code"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(kept.status.code(), Some(0));

    let index2 = tempfile::tempdir().unwrap();
    let dropped = aghist()
        .args(["search", "User", "--json", "--provider", "codex-cli"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index2.path())
        .output()
        .unwrap();
    assert_eq!(dropped.status.code(), Some(3));
}

#[test]
fn search_filter_role_restricts_hits_to_matching_role() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-search-role")
        .project("rolesearch")
        .user("apple banana")
        .assistant("cherry banana")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let user_only = aghist()
        .args(["search", "banana", "--json", "--role", "user"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(user_only.status.code(), Some(0));
    let stdout = String::from_utf8(user_only.stdout).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let rows = doc["hits"].as_array().expect("search JSON must have hits");
    assert_eq!(rows.len(), 1, "user-role filter should leave one hit");
    let snippet = rows[0]["snippet"].as_str().unwrap_or("");
    assert!(
        snippet.contains("apple"),
        "expected user message in hit, got snippet: {snippet}"
    );
}

#[test]
fn search_filter_has_tool_call_keeps_only_tool_messages() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-search-tool")
        .project("toolsearch")
        .user("search-keyword run me")
        .assistant_with_tool("ok", "Bash", r#"{"command":"search-keyword"}"#)
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "search-keyword", "--json", "--has-tool-call"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let rows = doc["hits"].as_array().expect("search JSON must have hits");
    assert_eq!(rows.len(), 1, "expected only the tool-using assistant hit");
}

#[test]
fn search_filter_since_drops_old_messages() {
    let fixture = common::fixtures::claude::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let output = aghist()
        .args([
            "search",
            "User",
            "--json",
            "--since",
            "2030-01-01T00:00:00Z",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn search_filter_project_substring_match_is_case_insensitive() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-search-proj")
        .project("AwesomeProject")
        .user("findme keyword")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let hit = aghist()
        .args(["search", "findme", "--json", "--project", "awesome"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();
    assert_eq!(hit.status.code(), Some(0));

    let index2 = tempfile::tempdir().unwrap();
    let miss = aghist()
        .args(["search", "findme", "--json", "--project", "nomatch"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index2.path())
        .output()
        .unwrap();
    assert_eq!(miss.status.code(), Some(3));
}
