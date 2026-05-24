use super::*;

#[test]
fn sources_pull_unknown_name_fails() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_dir = dir.path().join("cache");

    let output = aghist()
        .args(["sources", "pull", "ghost"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr_error_kind(&output), "source-not-found");
}

#[test]
fn sources_pull_rejects_invalid_name() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_dir = dir.path().join("cache");

    let output = aghist()
        .args(["sources", "pull", "../escape"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stderr_error_kind(&output), "usage");
}

#[test]
fn sources_pull_requires_name_or_all() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    aghist()
        .args(["sources", "add", "box", "--host", "h", "--path", "/p"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    let output = aghist()
        .args(["sources", "pull"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", dir.path().join("cache"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stderr_error_kind(&output), "usage");
}

#[test]
fn sources_pull_all_with_no_sources_fails() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let output = aghist()
        .args(["sources", "pull", "--all"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", dir.path().join("cache"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr_error_kind(&output), "source-not-found");
}

#[test]
fn sources_pull_all_validates_registry_before_rsync() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_dir = dir.path().join("cache");
    let args_log = dir.path().join("rsync-args.txt");
    let fake = write_fake_rsync(dir.path(), &args_log);
    std::fs::write(
        &config_path,
        r#"
[[sources]]
name = "valid"
host = "h"
path = "/p"

[[sources]]
name = "../escape"
host = "h"
path = "/p"
"#,
    )
    .unwrap();

    let output = aghist()
        .args(["sources", "pull", "--all"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_RSYNC_BIN", &fake)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr_error_kind(&output), "config-error");
    assert!(
        !args_log.exists(),
        "rsync must not run when any registry entry is invalid"
    );
}
