use super::aghist;
use super::common;
use super::common::cli;
use super::common::helpers::copy_dir_recursive;
use predicates::prelude::*;

/// Federated search: a registered remote source whose `data_dir` mirrors a
/// Claude Code home tree should contribute hits, tagged with the source name
/// in the `source` JSON field. Local hits stay tagged `"local"`.
#[test]
fn search_federates_across_local_and_remote_source_caches() {
    // Local fixture: a Claude session containing a unique token.
    let local = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("federated-local")
        .project("local-proj")
        .user("FEDERATED_TOKEN local message body")
        .done()
        .build();
    let home = local.base_path.parent().unwrap();

    // Remote source cache: a Claude tree under `<cache>/laptop/data/` with
    // its own session matching the same token.
    let cache_dir = tempfile::tempdir().unwrap();
    let remote_data = cache_dir.path().join("laptop").join("data");
    std::fs::create_dir_all(&remote_data).unwrap();

    let remote = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("federated-remote")
        .project("remote-proj")
        .user("FEDERATED_TOKEN remote message body")
        .done()
        .build();
    // The fixture builds at `<tmp>/.claude` — copy that into <cache>/laptop/data
    // so providers_rooted_at(remote_data) finds it via the `.claude` subpath.
    let remote_claude_src = remote.base_path.clone();
    let remote_claude_dst = remote_data.join(".claude");
    copy_dir_recursive(&remote_claude_src, &remote_claude_dst);

    // Register the remote source so federated discovery picks it up.
    let config_path = cache_dir.path().join("config.toml");
    aghist()
        .args([
            "sources",
            "add",
            "laptop",
            "--host",
            "laptop.local",
            "--path",
            "/home/x/.claude",
        ])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    let index_dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "FEDERATED_TOKEN", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", cache_dir.path())
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let doc = cli::output_stdout_json(&output);
    let hits = doc["hits"].as_array().expect("hits array");
    assert!(
        hits.len() >= 2,
        "expected hits from both local and remote sources, got: {hits:?}"
    );

    let by_session: std::collections::HashMap<&str, &str> = hits
        .iter()
        .map(|h| {
            (
                h["session_id"].as_str().unwrap(),
                h["source"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        by_session.get("federated-local"),
        Some(&"local"),
        "local session should be tagged 'local': {by_session:?}"
    );
    assert_eq!(
        by_session.get("federated-remote"),
        Some(&"laptop"),
        "remote session should be tagged with source name: {by_session:?}"
    );
}

#[test]
fn search_remote_sources_respect_enabled_provider_allowlist() {
    let empty_home = tempfile::tempdir().unwrap();
    let workdir = tempfile::tempdir().unwrap();
    let cache_dir = workdir.path().join("cache");
    let remote_data = cache_dir.join("laptop").join("data");
    std::fs::create_dir_all(&remote_data).unwrap();

    let remote = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("disabled-remote")
        .project("remote-proj")
        .user("DISABLED_REMOTE_TOKEN body")
        .done()
        .build();
    copy_dir_recursive(&remote.base_path, &remote_data.join(".claude"));

    let config_path = workdir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"
[[sources]]
name = "laptop"
host = "laptop.local"
path = "/home/x/.claude"
transport = "ssh"

[providers]
enabled = []
"#,
    )
    .unwrap();

    let index_dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "DISABLED_REMOTE_TOKEN", "--json"])
        .env("AGHIST_HOME", empty_home.path())
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(3),
        "disabled remote provider should not leak into search; stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Federated search must remain usable when a registered source has never
/// been pulled — its absence is logged as a `warning:` line on stderr but
/// search still surfaces local hits and exits 0.
#[test]
fn search_partial_failure_when_remote_cache_missing() {
    let local = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("partial-local")
        .project("local-proj")
        .user("PARTIAL_TOKEN local body")
        .done()
        .build();
    let home = local.base_path.parent().unwrap();

    let workdir = tempfile::tempdir().unwrap();
    let config_path = workdir.path().join("config.toml");
    let cache_dir = workdir.path().join("cache");

    // Register a source but never pull — its data_dir does not exist.
    aghist()
        .args(["sources", "add", "ghost", "--host", "g", "--path", "/p"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    let index_dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "PARTIAL_TOKEN", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    // Local hit must still surface — partial failure must not be fatal.
    assert_eq!(output.status.code(), Some(0));

    let stderr = cli::output_stderr(&output);
    assert!(
        stderr.contains("warning:") && stderr.contains("ghost"),
        "expected warning about missing 'ghost' cache, got: {stderr}"
    );

    let doc = cli::output_stdout_json(&output);
    let hits = doc["hits"].as_array().unwrap();
    assert!(!hits.is_empty(), "local hit should still appear");
    assert_eq!(hits[0]["source"], "local");
}
#[test]
fn search_help_exits_zero() {
    aghist()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Search indexed sessions"));
}
#[test]
fn search_requires_query_argument() {
    aghist()
        .arg("search")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "search requires a query (positional, --query-file, or --stdin)",
        ));
}
#[test]
fn search_query_file_and_stdin_are_mutually_exclusive() {
    aghist()
        .args(["search", "--query-file", "/tmp/q.txt", "--stdin"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}
#[test]
fn search_positional_and_query_file_are_mutually_exclusive() {
    aghist()
        .args(["search", "hello", "--query-file", "/tmp/q.txt"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}
#[test]
fn search_stdin_reads_query_from_standard_input() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "--stdin", "--json"])
        .env("AGHIST_HOME", dir.path())
        .write_stdin("{some braces} \"and quotes\"\n")
        .output()
        .unwrap();
    // No data, so we expect EXIT_EMPTY (3) — but critically, NOT EXIT_USAGE (2)
    // and NOT a clap parse error. The query was accepted from stdin.
    assert_ne!(
        output.status.code(),
        Some(2),
        "stdin query should be accepted; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn search_query_file_reads_query_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let qfile = dir.path().join("q.txt");
    std::fs::write(&qfile, "{a} \"b\"\n").unwrap();
    let output = aghist()
        .args(["search", "--query-file"])
        .arg(&qfile)
        .args(["--json"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_ne!(
        output.status.code(),
        Some(2),
        "query-file should be accepted; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn search_query_file_missing_path_emits_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args([
            "search",
            "--query-file",
            "/nonexistent/path/does/not/exist.txt",
            "--json",
        ])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to read query file"),
        "expected io-error envelope, got: {stderr}"
    );
}
#[test]
fn search_query_file_dash_reads_from_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "--query-file", "-", "--json"])
        .env("AGHIST_HOME", dir.path())
        .write_stdin("test query\n")
        .output()
        .unwrap();
    assert_ne!(
        output.status.code(),
        Some(2),
        "--query-file - should read from stdin; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn search_empty_stdin_reports_empty_query() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "--stdin", "--json"])
        .env("AGHIST_HOME", dir.path())
        .write_stdin("   \n\n")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("search query is empty"),
        "expected empty-query envelope, got: {stderr}"
    );
}
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
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "User", "--json", "--debug-search"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let doc = cli::output_stdout_json(&output);
    let arr = doc["hits"].as_array().expect("expected JSON array of hits");
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
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "User", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let doc = cli::output_stdout_json(&output);
    let arr = doc["hits"].as_array().expect("expected array");
    assert!(!arr.is_empty());
    assert!(
        arr[0].get("explanation").is_none(),
        "default search must NOT include 'explanation' field"
    );
}
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
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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
    assert_eq!(
        page1.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&page1.stderr)
    );
    let doc1 = cli::output_stdout_json(&page1);
    assert_eq!(doc1["hits"].as_array().unwrap().len(), 1);
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
    assert_eq!(
        page2.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&page2.stderr)
    );
    let doc2 = cli::output_stdout_json(&page2);
    assert_eq!(doc2["hits"].as_array().unwrap().len(), 1);

    let first = doc1["hits"][0]["message_id"].as_str().unwrap();
    let second = doc2["hits"][0]["message_id"].as_str().unwrap();
    assert_ne!(first, second, "cursor page repeated the same hit");
}
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
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "User", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let doc = cli::output_stdout_json(&output);
    assert_eq!(
        doc["meta"]["engine"], "lexical",
        "default search must report engine=lexical, got: {}",
        doc["meta"]
    );
}
#[test]
fn search_hybrid_weight_falls_open_to_lexical_without_embeddings() {
    // Without the `embeddings` feature compiled in (or with no consent file
    // and no store), --hybrid-weight must NOT crash or fail — it falls open
    // to lexical-only and reports `engine: lexical` in meta. This is the
    // core "fail open" guarantee from ahist-y3o.4.2.
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "User", "--hybrid-weight", "0.5", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "hybrid search must succeed even without embeddings; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let doc = cli::output_stdout_json(&output);
    let arr = doc["hits"].as_array().expect("expected hits array");
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
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .args(["search", "User", "--hybrid-weight", "0.0", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let doc = cli::output_stdout_json(&output);
    assert_eq!(
        doc["meta"]["engine"], "lexical",
        "hybrid_weight=0 must not engage hybrid path"
    );
}
#[test]
fn search_json_output_wraps_hits_in_meta_envelope() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
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

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let doc = cli::output_stdout_json(&output);
    let hits = doc["hits"].as_array().expect("search JSON hits array");
    assert_eq!(hits.len(), 1, "unique fixture token should produce one hit");
    assert!(doc["meta"].is_object(), "search JSON must include 'meta'");
    assert!(doc["meta"]["total"].is_number());
    assert_eq!(doc["meta"]["engine"], "lexical");

    let hit = &hits[0];
    assert_eq!(hit["kind"], "message");
    assert_eq!(hit["source"], "local");
    assert_eq!(hit["session_id"], "search-json-envelope");
    assert!(hit["ref"].as_str().unwrap().ends_with("#1"));
}
#[test]
fn search_watch_help_documents_flags() {
    aghist()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--watch"))
        .stdout(predicate::str::contains("--watch-interval-ms"))
        .stdout(predicate::str::contains("--watch-iterations"));
}
#[test]
fn search_watch_emits_ndjson_one_per_line_for_existing_matches() {
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let output = aghist()
        .args([
            "search",
            "User",
            "--watch",
            "--watch-interval-ms",
            "10",
            "--watch-iterations",
            "1",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = cli::output_stdout(&output);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert!(
        !lines.is_empty(),
        "expected at least one NDJSON hit, got: {stdout:?}"
    );
    for line in &lines {
        let row: serde_json::Value =
            serde_json::from_str(line).expect("each watch line must be valid JSON");
        assert!(row["session_id"].is_string());
        assert!(row["message_id"].is_string());
        assert!(row["snippet"].is_string());
        assert!(row.get("score").is_some());
        // Watch NDJSON must NOT wrap rows in an array envelope.
        assert!(!line.trim_start().starts_with('['));
    }
}
#[test]
fn search_watch_dedups_hits_across_polls() {
    // Same fixture across 3 polls — every hit should appear exactly once.
    let fixture = common::fixtures::claude_single_session(4);
    let home = fixture.base_path.parent().unwrap();
    let index = tempfile::tempdir().unwrap();

    let output = aghist()
        .args([
            "search",
            "User",
            "--watch",
            "--watch-interval-ms",
            "10",
            "--watch-iterations",
            "3",
        ])
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = cli::output_stdout(&output);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

    let keys: Vec<(String, String)> = lines
        .iter()
        .map(|l| {
            let row: serde_json::Value = serde_json::from_str(l).unwrap();
            (
                row["session_id"].as_str().unwrap().to_string(),
                row["message_id"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let unique: std::collections::HashSet<_> = keys.iter().cloned().collect();
    assert_eq!(
        keys.len(),
        unique.len(),
        "watch must emit each (session_id, message_id) at most once across polls; got {} lines / {} unique",
        keys.len(),
        unique.len()
    );
}
#[test]
fn search_watch_requires_query() {
    let dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "--watch", "--watch-iterations", "1"])
        .env("AGHIST_HOME", dir.path())
        .env("AGHIST_INDEX_DIR", dir.path().join("idx"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = cli::output_stderr(&output);
    assert!(
        stderr.contains("search requires a query"),
        "expected usage envelope, got: {stderr}"
    );
}
#[test]
fn search_params_invokes_query() {
    let fixture = common::fixtures::ClaudeFixtureBuilder::new()
        .add_session("session-search-params")
        .project("search-params-project")
        .user("uniqueneedlephrase")
        .assistant("answer")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    // Build the index first so search has something to find.
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
    let hits = parsed["hits"]
        .as_array()
        .expect("search JSON must wrap rows in 'hits'");
    assert!(
        !hits.is_empty(),
        "expected at least one hit for the unique phrase"
    );
}
