use super::super::aghist;
use super::super::common;
use super::super::common::cli;

#[test]
fn list_federates_remote_sources_and_paginates_reused_session_ids() {
    let local = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("shared-list-id")
        .project("local-proj")
        .user("local session without tool call")
        .done()
        .build();
    let home = local.base_path.parent().unwrap();

    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("shared-list-id")
        .project("remote-proj")
        .user("remote session")
        .assistant_with_tool("using a tool", "Read", r#"{"file_path":"/tmp/remote.txt"}"#)
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let page1 = aghist()
        .args(["--list", "--json", "--limit", "1"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .output()
        .unwrap();
    cli::assert_success(&page1);
    let doc1 = cli::output_stdout_json(&page1);
    assert_eq!(doc1["meta"]["total"], 2);
    assert_eq!(cli::json_array(&doc1, "sessions").len(), 1);
    let cursor = cli::json_str(&doc1["meta"], "next_cursor");

    let page2 = aghist()
        .args(["--list", "--json", "--limit", "1", "--cursor", cursor])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .output()
        .unwrap();
    cli::assert_success(&page2);
    let doc2 = cli::output_stdout_json(&page2);
    assert_eq!(cli::json_array(&doc2, "sessions").len(), 1);
    assert!(doc2["meta"]["next_cursor"].is_null());

    let sources: std::collections::HashSet<String> = cli::json_array(&doc1, "sessions")
        .iter()
        .chain(cli::json_array(&doc2, "sessions").iter())
        .map(|row| cli::json_str(row, "source").to_string())
        .collect();
    assert_eq!(
        sources,
        std::collections::HashSet::from(["local".to_string(), "laptop".to_string()])
    );

    let tool_filtered = aghist()
        .args(["--list", "--json", "--has-tool-call"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .output()
        .unwrap();
    cli::assert_success(&tool_filtered);
    let filtered = cli::output_stdout_json(&tool_filtered);
    let rows = cli::json_array(&filtered, "sessions");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["source"], "laptop");
    assert_eq!(rows[0]["id"], "shared-list-id");
}

#[test]
fn list_limit_caps_returned_sessions_and_emits_next_cursor() {
    let fixture = common::fixtures::claude::claude_multi_session(5, 2);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["--list", "--json", "--limit", "2"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    cli::assert_success(&output);
    let doc = cli::output_stdout_json(&output);
    let sessions = cli::json_array(&doc, "sessions");
    assert_eq!(sessions.len(), 2, "limit must cap returned rows");
    assert_eq!(doc["meta"]["total"], 5, "total reflects all matching rows");
    assert!(
        doc["meta"]["next_cursor"].is_string(),
        "next_cursor must be set when more results exist"
    );
}

#[test]
fn list_cursor_resumes_after_prior_page_and_paginates_to_completion() {
    let fixture = common::fixtures::claude::claude_multi_session(5, 2);
    let home = fixture.base_path.parent().unwrap();

    let page1 = aghist()
        .args(["--list", "--json", "--limit", "2"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    cli::assert_success(&page1);
    let doc1 = cli::output_stdout_json(&page1);
    let cursor1 = cli::json_str(&doc1["meta"], "next_cursor").to_string();
    let ids1: Vec<String> = cli::json_array(&doc1, "sessions")
        .iter()
        .map(|s| cli::json_str(s, "id").to_string())
        .collect();

    let page2 = aghist()
        .args(["--list", "--json", "--limit", "2", "--cursor", &cursor1])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    cli::assert_success(&page2);
    let doc2 = cli::output_stdout_json(&page2);
    let ids2: Vec<String> = cli::json_array(&doc2, "sessions")
        .iter()
        .map(|s| cli::json_str(s, "id").to_string())
        .collect();
    assert_eq!(ids2.len(), 2);
    let cursor2 = cli::json_str(&doc2["meta"], "next_cursor").to_string();

    let page3 = aghist()
        .args(["--list", "--json", "--limit", "2", "--cursor", &cursor2])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    cli::assert_success(&page3);
    let doc3 = cli::output_stdout_json(&page3);
    let ids3: Vec<String> = cli::json_array(&doc3, "sessions")
        .iter()
        .map(|s| cli::json_str(s, "id").to_string())
        .collect();
    assert_eq!(ids3.len(), 1);
    assert!(
        doc3["meta"]["next_cursor"].is_null(),
        "final page must not advertise another cursor"
    );

    let all_ids: Vec<&String> = ids1.iter().chain(ids2.iter()).chain(ids3.iter()).collect();
    let unique: std::collections::HashSet<&&String> = all_ids.iter().collect();
    assert_eq!(
        unique.len(),
        all_ids.len(),
        "pagination must not duplicate ids"
    );
    assert_eq!(all_ids.len(), 5, "all 5 sessions must be visited");
}

#[test]
fn list_invalid_cursor_returns_usage_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["--list", "--cursor", "not-a-real-cursor!!!"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let envelope = cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
}

#[test]
fn list_rejects_zero_limit_flag() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["--list", "--limit", "0"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let envelope = cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("list limit must be at least 1"),
        "unexpected error envelope: {envelope:#}"
    );
}

#[test]
fn list_cursor_requires_list_flag() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["--cursor", "abc"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let envelope = cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
}
