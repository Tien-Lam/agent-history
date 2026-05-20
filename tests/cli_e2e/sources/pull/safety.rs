use std::os::unix::fs::symlink;

use super::*;

#[test]
fn sources_pull_refuses_symlinked_cache_root_before_rsync() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_dir = dir.path().join("cache-link");
    let escape_dir = dir.path().join("escape-target");
    let args_log = dir.path().join("rsync-args.txt");
    let fake = write_fake_rsync(dir.path(), &args_log);
    std::fs::create_dir_all(&escape_dir).unwrap();

    aghist()
        .args(["sources", "add", "box", "--host", "h", "--path", "/p"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();
    symlink(&escape_dir, &cache_dir).unwrap();

    let output = aghist()
        .args(["sources", "pull", "box"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_RSYNC_BIN", &fake)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr_error_kind(&output), "unsafe-cache-dir");
    assert!(
        !args_log.exists(),
        "rsync must not run when cache root is a symlink"
    );
    assert!(
        !escape_dir.join("box").exists(),
        "pull must not write through symlinked cache root"
    );
}

#[test]
fn sources_pull_refuses_symlinked_source_cache_dir_before_rsync() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_dir = dir.path().join("cache");
    let escape_dir = dir.path().join("escape-target");
    let args_log = dir.path().join("rsync-args.txt");
    let fake = write_fake_rsync(dir.path(), &args_log);
    std::fs::create_dir_all(&escape_dir).unwrap();

    aghist()
        .args(["sources", "add", "box", "--host", "h", "--path", "/p"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();
    std::fs::create_dir_all(&cache_dir).unwrap();
    symlink(&escape_dir, cache_dir.join("box")).unwrap();

    let output = aghist()
        .args(["sources", "pull", "box"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_RSYNC_BIN", &fake)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr_error_kind(&output), "unsafe-cache-dir");
    assert!(
        !args_log.exists(),
        "rsync must not run when cache root is a symlink"
    );
    assert!(
        !escape_dir.join("sample.jsonl").exists(),
        "pull must not write through symlinked cache root"
    );
}

#[test]
fn sources_pull_refuses_symlinked_data_dir_before_rsync() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_dir = dir.path().join("cache");
    let source_dir = cache_dir.join("box");
    let escape_dir = dir.path().join("escape-target");
    let args_log = dir.path().join("rsync-args.txt");
    let fake = write_fake_rsync(dir.path(), &args_log);
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&escape_dir).unwrap();

    aghist()
        .args(["sources", "add", "box", "--host", "h", "--path", "/p"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();
    symlink(&escape_dir, source_dir.join("data")).unwrap();

    let output = aghist()
        .args(["sources", "pull", "box"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_RSYNC_BIN", &fake)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stderr_error_kind(&output), "unsafe-cache-dir");
    assert!(
        !args_log.exists(),
        "rsync must not run when data dir is a symlink"
    );
    assert!(
        !escape_dir.join("sample.jsonl").exists(),
        "pull must not write through symlinked data dir"
    );
}
