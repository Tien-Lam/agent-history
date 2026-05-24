use super::super::aghist;
use super::super::common;
use super::super::common::cli;
use predicates::prelude::*;

#[test]
fn reindex_flag_clears_index() {
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
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["added"], 0);
    assert_eq!(parsed["updated"], 0);
    assert_eq!(parsed["unchanged"], 0);
    assert_eq!(parsed["sessions_total"], 0);
    assert_eq!(parsed["messages_indexed"], 0);
}

#[test]
fn index_idempotent_second_run_reports_unchanged() {
    let fixture = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("session-index-test")
        .project("idx-project")
        .user("Hello")
        .assistant("World")
        .done()
        .build();
    let home = fixture.base_path.parent().unwrap();
    let index_dir = tempfile::tempdir().unwrap();

    let first = aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let first_json = cli::assert_stdout_json(&first);
    assert_eq!(first_json["status"], "ok");
    assert_eq!(
        first_json["added"], 1,
        "first run should classify session as 'added'"
    );
    assert_eq!(first_json["updated"], 0);
    assert_eq!(first_json["unchanged"], 0);
    assert!(first_json["messages_indexed"].as_u64().unwrap() >= 1);

    let second = aghist()
        .arg("index")
        .env("AGHIST_HOME", home)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .success();
    let second_json = cli::assert_stdout_json(&second);
    assert_eq!(second_json["status"], "ok");
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
    let claude = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("claude-only")
        .project("c-proj")
        .user("hi")
        .assistant("hello")
        .done()
        .build();
    let codex = common::fixtures::codex::codex_single_session(2);

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
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["providers"], serde_json::json!(["claude-code"]));
    assert_eq!(
        parsed["sessions_total"], 1,
        "only Claude session is in scope"
    );
    assert_eq!(parsed["added"], 1);
}

#[test]
fn index_provider_filter_includes_remote_cache_without_local_provider() {
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
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
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["providers"], serde_json::json!(["claude-code"]));
    assert_eq!(parsed["sessions_total"], 1);
    assert_eq!(parsed["added"], 1);
    assert!(
        parsed["messages_indexed"].as_u64().unwrap() >= 1,
        "remote session messages should be indexed: {parsed}"
    );
}

#[test]
fn index_remote_source_discovery_error_exits_one_with_partial_summary() {
    let home = tempfile::tempdir().unwrap();
    let workdir = tempfile::tempdir().unwrap();
    let config_path = workdir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"[[sources]]
name = "laptop"
host = "laptop.local"
path = "/home/x/.claude"
transport = "ssh"
"#,
    )
    .unwrap();
    let cache_dir = workdir.path().join("cache");
    let index_dir = tempfile::tempdir().unwrap();

    let output = aghist()
        .arg("index")
        .env("AGHIST_HOME", home.path())
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .code(1);

    let parsed = cli::assert_stdout_json(&output);
    assert_eq!(parsed["status"], "partial");
    assert_eq!(parsed["sessions_total"], 0);
    assert_eq!(parsed["errors"][0]["source"], "laptop");
    assert!(parsed["errors"][0]["error"]
        .as_str()
        .unwrap()
        .contains("cache missing"));
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
    assert!(
        matches!(status, "disabled" | "awaiting-consent" | "enabled"),
        "unexpected embeddings status: {status:?}"
    );
}
