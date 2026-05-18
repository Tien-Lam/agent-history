use super::aghist;
use super::common;
use super::common::cli;
use predicates::prelude::*;

#[test]
fn list_with_no_data_exits_three_for_empty() {
    // Tests run under assert_cmd; stdout is piped, so --list auto-emits NDJSON.
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    cli::assert_empty(&output);
    let stdout = cli::output_stdout(&output);
    // Empty list emits zero session rows; the trailing `{"meta": ...}` row
    // is always present so streaming consumers can detect end-of-stream.
    let session_rows: Vec<&str> = stdout
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("\"meta\""))
        .collect();
    assert!(
        session_rows.is_empty(),
        "empty list under NDJSON should emit zero session rows, got: {session_rows:?}"
    );
    assert!(
        stdout.contains("\"total\":0"),
        "trailing meta row must report total=0, got: {stdout:?}"
    );
}
#[test]
fn list_with_generated_claude_fixtures() {
    let fixture = common::fixtures::claude_single_session(4);
    // base_path is {tmpdir}/.claude, AGHIST_HOME should be the parent.
    // Under non-TTY (piped stdout), --list emits NDJSON: one row per session.
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    cli::assert_success(&output);
    let rows = cli::output_ndjson_session_rows(&output);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["provider"], "claude-code");
    assert_eq!(rows[0]["message_count"], 4);
}
#[test]
fn list_with_data_exits_zero() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    aghist()
        .arg("--list")
        .env("AGHIST_HOME", home)
        .assert()
        .code(0);
}
#[test]
fn list_with_multiple_providers() {
    let claude = common::fixtures::claude_single_session(2);
    let codex = common::fixtures::codex_single_session(2);

    let home = common::helpers::FixtureHome::new();
    home.add_claude(&claude);
    home.add_codex(&codex);

    // Under non-TTY, --list emits NDJSON. Assert both providers appear.
    let output = aghist()
        .arg("--list")
        .env("AGHIST_HOME", home.path())
        .output()
        .unwrap();
    cli::assert_success(&output);
    let providers: std::collections::HashSet<String> = cli::output_ndjson_session_rows(&output)
        .iter()
        .filter_map(|row| row["provider"].as_str().map(str::to_string))
        .collect();
    assert!(providers.contains("claude-code"));
    assert!(providers.contains("codex-cli"));
}

#[test]
fn list_federates_remote_sources_and_paginates_reused_session_ids() {
    let local = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("shared-list-id")
        .project("local-proj")
        .user("local session without tool call")
        .done()
        .build();
    let home = local.base_path.parent().unwrap();

    let remote = common::fixtures::ClaudeFixtureBuilder::new()
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
fn reindex_flag_clears_index() {
    // --reindex paired with --list against an empty home: the reindex side
    // should succeed (clear the index), and --list reports the empty exit
    // code 3 — the test asserts both signals.
    let dir = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();
    aghist()
        .arg("--reindex")
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Search index cleared"));
}
#[test]
fn index_no_data_emits_zero_counts_json() {
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .arg("index")
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();

    let parsed = cli::assert_stdout_json(&output);
    assert_eq!(parsed["added"], 0);
    assert_eq!(parsed["updated"], 0);
    assert_eq!(parsed["unchanged"], 0);
    assert_eq!(parsed["sessions_total"], 0);
    assert_eq!(parsed["messages_indexed"], 0);
}
#[test]
fn index_idempotent_second_run_reports_unchanged() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-index-test")
        .project("idx-project")
        .user("Hello")
        .assistant("World")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    // First run: should add the session
    let first = aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let first_json = cli::assert_stdout_json(&first);
    assert_eq!(
        first_json["added"], 1,
        "first run should classify session as 'added'"
    );
    assert_eq!(first_json["updated"], 0);
    assert_eq!(first_json["unchanged"], 0);
    assert!(first_json["messages_indexed"].as_u64().unwrap() >= 1);

    // Second run: same fixture, should be unchanged
    let second = aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let second_json = cli::assert_stdout_json(&second);
    assert_eq!(second_json["added"], 0);
    assert_eq!(second_json["updated"], 0);
    assert_eq!(
        second_json["unchanged"], 1,
        "second run should report 1 unchanged"
    );
    assert_eq!(second_json["messages_indexed"], 0);
}
#[test]
fn index_provider_filter_restricts_scope() {
    // Build a home with both Claude and Codex sessions; --provider claude-code should
    // index only the Claude one.
    let claude = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("claude-only")
        .project("c-proj")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let codex = common::fixtures::codex_single_session(2);

    let home = common::helpers::FixtureHome::new();
    home.add_claude(&claude);
    home.add_codex(&codex);
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["index", "--provider", "claude-code"])
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let parsed = cli::assert_stdout_json(&output);
    assert_eq!(parsed["providers"], serde_json::json!(["claude-code"]));
    assert_eq!(
        parsed["sessions_total"], 1,
        "only Claude session is in scope"
    );
    assert_eq!(parsed["added"], 1);
}

#[test]
fn index_provider_filter_includes_remote_cache_without_local_provider() {
    let remote = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("remote-index-only")
        .project("remote-proj")
        .user("REMOTE_INDEX_TOKEN remote message body")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let index_dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["index", "--provider", "claude-code"])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();

    let parsed = cli::assert_stdout_json(&output);
    assert_eq!(parsed["providers"], serde_json::json!(["claude-code"]));
    assert_eq!(parsed["sessions_total"], 1);
    assert_eq!(parsed["added"], 1);
    assert!(
        parsed["messages_indexed"].as_u64().unwrap() >= 1,
        "remote session messages should be indexed: {parsed}"
    );
}

#[test]
fn index_unknown_provider_emits_usage_envelope_and_exits_two() {
    let home = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["index", "--provider", "bogus"])
        .env("AGHIST_HOME", home.path())
        .assert()
        .code(2);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("bogus"));
}
#[test]
fn list_json_emits_single_object_with_sessions_array() {
    let fixture = common::fixtures::claude_single_session(3);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["--list", "--json"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    cli::assert_success(&output);
    let doc = cli::output_stdout_json(&output);
    let sessions = cli::json_array(&doc, "sessions");
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0]["id"].is_string());
    assert!(sessions[0]["provider"].is_string());
    assert!(sessions[0]["started_at"].is_string());
    assert_eq!(sessions[0]["message_count"], 3);
}
#[test]
fn list_ndjson_emits_one_session_per_line() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    let output = aghist()
        .args(["--list", "--ndjson"])
        .env("AGHIST_HOME", home)
        .output()
        .unwrap();
    cli::assert_success(&output);
    let parsed = cli::output_ndjson_values(&output);
    // One session row + one trailing `{"meta": ...}` envelope row.
    assert_eq!(parsed.len(), 2);
    let session = &parsed[0];
    assert!(session["id"].is_string());
    assert_eq!(session["message_count"], 2);
    // NDJSON session rows must NOT be wrapped in a `sessions` envelope.
    assert!(session.get("sessions").is_none());
    let meta_row = &parsed[1];
    assert!(
        meta_row.get("meta").is_some(),
        "last NDJSON row must be the meta envelope, got: {meta_row}"
    );
    assert_eq!(meta_row["meta"]["total"], 1);
}
#[test]
fn list_json_empty_returns_three_with_empty_array() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["--list", "--json"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    cli::assert_empty(&output);
    let doc = cli::output_stdout_json(&output);
    assert_eq!(cli::json_array(&doc, "sessions").len(), 0);
}
#[test]
fn list_rejects_json_and_ndjson_together() {
    let dir = tempfile::tempdir().unwrap();
    let assert = aghist()
        .args(["--list", "--json", "--ndjson"])
        .env("AGHIST_HOME", dir.path())
        .assert()
        .code(2);
    let envelope = cli::assert_stderr_error(&assert);
    assert_eq!(envelope["error"]["kind"], "usage");
}
#[test]
fn list_limit_caps_returned_sessions_and_emits_next_cursor() {
    let fixture = common::fixtures::claude_multi_session(5, 2);
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
    let fixture = common::fixtures::claude_multi_session(5, 2);
    let home = fixture.base_path.parent().unwrap();

    // First page.
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

    // Second page (resume).
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

    // Third (final) page — has the last session and no further cursor.
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

    // No id appears across pages (no skips, no duplicates).
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
#[test]
fn index_params_force_flag() {
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();
    let body = serde_json::json!({"force": true}).to_string();

    aghist()
        .args(["index", "--params", &body])
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
}
#[test]
fn index_params_unknown_provider_slug_emits_usage() {
    let home = tempfile::tempdir().unwrap();
    let body = serde_json::json!({"provider": "bogus"}).to_string();
    let assert = aghist()
        .args(["index", "--params", &body])
        .env("AGHIST_HOME", home.path())
        .assert()
        .code(1);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
}
#[test]
fn index_help_documents_accept_download_flag() {
    aghist()
        .args(["index", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--accept-download"))
        .stdout(predicate::str::contains("AllMiniLML6V2"));
}
#[test]
fn index_summary_includes_embeddings_block() {
    // Without the `embeddings` cargo feature compiled in, the summary should
    // surface that explicitly so callers (and humans) know nothing semantic
    // happened — even when --accept-download is passed.
    let home = tempfile::tempdir().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["index", "--accept-download"])
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();

    let parsed = cli::assert_stdout_json(&output);
    let block = &parsed["embeddings"];
    assert!(block.is_object(), "expected embeddings object, got {block}");
    let status = block["status"].as_str().unwrap_or("");
    // The lean default build reports "disabled"; a feature build reports
    // "awaiting-consent" or "enabled". Accept any of those — the contract is
    // that the field exists and tells the caller what happened.
    assert!(
        matches!(status, "disabled" | "awaiting-consent" | "enabled"),
        "unexpected embeddings status: {status:?}"
    );
}
