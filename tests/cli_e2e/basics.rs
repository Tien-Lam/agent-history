use super::aghist;
use super::common::cli;
use predicates::prelude::*;

#[test]
fn help_flag_exits_zero() {
    aghist()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Browse and search AI agent conversation history",
        ));
}
#[test]
fn version_flag_exits_zero() {
    aghist()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("aghist"));
}
#[test]
fn unknown_subcommand_exits_two_with_usage_envelope() {
    let assert = aghist().arg("totally-unknown").assert().code(2);
    let parsed = cli::assert_stderr_error(&assert);
    assert_eq!(parsed["error"]["kind"], "usage");
}
#[test]
fn update_help_exits_zero() {
    aghist()
        .args(["update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Update a self-managed release binary to the latest GitHub release",
        ));
}
#[test]
fn uninstall_help_exits_zero() {
    aghist()
        .args(["uninstall", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Remove a self-managed release binary and data",
        ));
}

#[test]
fn update_from_build_tree_is_rejected_before_network() {
    let assert = aghist().arg("update").assert().failure();
    let parsed = cli::assert_stderr_error(&assert);

    assert_eq!(parsed["error"]["kind"], "unsupported-install-method");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Cargo build directory"));
}

#[test]
fn uninstall_from_build_tree_is_rejected_before_prompt() {
    let assert = aghist().arg("uninstall").assert().failure();
    let parsed = cli::assert_stderr_error(&assert);

    assert_eq!(parsed["error"]["kind"], "unsupported-install-method");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Cargo build directory"));
}

#[test]
fn reindex_does_not_clear_index_on_usage_error() {
    let index_dir = tempfile::tempdir().unwrap();
    let manifest = index_dir.path().join("manifest.json");
    std::fs::write(&manifest, "{}").unwrap();

    aghist()
        .args(["--reindex", "--json", "--ndjson", "--list"])
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .assert()
        .code(2);

    assert!(
        manifest.exists(),
        "--reindex must not mutate the index before usage validation succeeds"
    );
}

#[test]
fn reindex_does_not_clear_index_on_config_error() {
    let root = tempfile::tempdir().unwrap();
    let config_path = root.path().join("bad-config.toml");
    let index_dir = root.path().join("idx");
    let manifest = index_dir.join("manifest.json");
    std::fs::create_dir_all(&index_dir).unwrap();
    std::fs::write(&manifest, "{}").unwrap();
    std::fs::write(&config_path, "[providers\n").unwrap();

    aghist()
        .args(["--reindex", "--list"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_INDEX_DIR", &index_dir)
        .assert()
        .failure();

    assert!(
        manifest.exists(),
        "--reindex must not mutate the index before config validation succeeds"
    );
}

#[test]
fn reindex_reports_index_open_failure() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let index_path = root.path().join("not-a-directory");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(&index_path, "file, not a directory").unwrap();

    let assert = aghist()
        .args(["--reindex", "--list"])
        .env("AGHIST_HOME", &home)
        .env("AGHIST_INDEX_DIR", &index_path)
        .assert()
        .failure();
    let parsed = cli::assert_stderr_error(&assert);

    assert_eq!(parsed["error"]["kind"], "index-error");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("failed to open search index"));
}
