use super::super::aghist;
use super::super::common;
use super::super::common::cli;

/// Federated search: a registered remote source whose `data_dir` mirrors a
/// Claude Code home tree should contribute hits, tagged with the source name
/// in the `source` JSON field. Local hits stay tagged `"local"`.
#[test]
fn search_federates_across_local_and_remote_source_caches() {
    let local = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("federated-local")
        .project("local-proj")
        .user("FEDERATED_TOKEN local message body")
        .done()
        .build();
    let home = local.base_path.parent().unwrap();

    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("federated-remote")
        .project("remote-proj")
        .user("FEDERATED_TOKEN remote message body")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source(&remote.base_path);

    let index_dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "FEDERATED_TOKEN", "--json"])
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    cli::assert_success(&output);

    let doc = cli::output_stdout_json(&output);
    let hits = cli::json_array(&doc, "hits");
    assert!(
        hits.len() >= 2,
        "expected hits from both local and remote sources, got: {hits:?}"
    );

    let by_session: std::collections::HashMap<&str, &str> = hits
        .iter()
        .map(|h| (cli::json_str(h, "session_id"), cli::json_str(h, "source")))
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
    let remote = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("disabled-remote")
        .project("remote-proj")
        .user("DISABLED_REMOTE_TOKEN body")
        .done()
        .build();
    let source = common::helpers::laptop_remote_source_with_config(
        &remote.base_path,
        r"
[providers]
enabled = []
",
    );

    let index_dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .args(["search", "DISABLED_REMOTE_TOKEN", "--json"])
        .env("AGHIST_HOME", source.empty_home.path())
        .env("AGHIST_CONFIG", &source.config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &source.cache_dir)
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();

    cli::assert_empty(&output);
}

/// Federated search must remain usable when a registered source has never
/// been pulled. Its absence is logged as a `warning:` line on stderr, but
/// search still surfaces local hits and exits 0.
#[test]
fn search_partial_failure_when_remote_cache_missing() {
    let local = common::fixtures::claude::ClaudeFixtureBuilder::new()
        .add_session("partial-local")
        .project("local-proj")
        .user("PARTIAL_TOKEN local body")
        .done()
        .build();
    let home = local.base_path.parent().unwrap();

    let workdir = tempfile::tempdir().unwrap();
    let config_path = workdir.path().join("config.toml");
    let cache_dir = workdir.path().join("cache");

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
    cli::assert_success(&output);

    let stderr = cli::output_stderr(&output);
    assert!(
        stderr.contains("warning:") && stderr.contains("ghost"),
        "expected warning about missing 'ghost' cache, got: {stderr}"
    );

    let doc = cli::output_stdout_json(&output);
    let hits = cli::json_array(&doc, "hits");
    assert!(!hits.is_empty(), "local hit should still appear");
    assert_eq!(hits[0]["source"], "local");
}
