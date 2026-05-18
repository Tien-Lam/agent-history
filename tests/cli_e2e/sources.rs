use super::aghist;
use super::common;

#[test]
fn sources_emits_json_with_provider_rows() {
    let fixture = common::fixtures::claude_single_session(3);
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args(["sources"])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let sources = parsed["sources"].as_array().expect("sources array");
    let claude_row = sources
        .iter()
        .find(|r| r["provider"] == "claude-code")
        .expect("claude-code source row");
    assert!(claude_row["session_count"].as_u64().unwrap() >= 1);
    assert!(claude_row["paths"].as_array().is_some());
    assert!(parsed["index"]["dir"].is_string());
}
#[test]
fn sources_ndjson_one_row_per_provider() {
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();

    let assert = aghist()
        .args(["--ndjson", "sources"])
        .env("AGHIST_HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert!(!lines.is_empty(), "expected at least one NDJSON line");
    for line in &lines {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(row["provider"].is_string());
        assert!(row["session_count"].is_number());
    }
}
#[test]
fn sources_empty_home_exits_three() {
    let dir = tempfile::tempdir().unwrap();
    // Empty home: no providers detected — Sources should exit 3 (success-but-empty).
    let output = aghist()
        .args(["sources"])
        .env("AGHIST_HOME", dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
}
#[test]
fn sources_list_empty_registry_exits_three() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let output = aghist()
        .args(["sources", "list"])
        .env("AGHIST_CONFIG", &config_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let arr = parsed["sources"].as_array().expect("sources array");
    assert!(arr.is_empty());
}
#[test]
fn sources_add_then_list_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    aghist()
        .args([
            "sources",
            "add",
            "laptop",
            "--host",
            "user@laptop.local",
            "--path",
            "/home/user/.claude",
            "--transport",
            "rsync",
        ])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    assert!(config_path.exists(), "config.toml should be created");
    let toml_text = std::fs::read_to_string(&config_path).unwrap();
    assert!(toml_text.contains("[[sources]]"), "{}", toml_text);
    assert!(toml_text.contains("laptop"));
    assert!(toml_text.contains("rsync"));

    let assert = aghist()
        .args(["sources", "list"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let arr = parsed["sources"].as_array().expect("sources array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "laptop");
    assert_eq!(arr[0]["host"], "user@laptop.local");
    assert_eq!(arr[0]["path"], "/home/user/.claude");
    assert_eq!(arr[0]["transport"], "rsync");
}
#[test]
fn sources_add_duplicate_name_fails() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    aghist()
        .args(["sources", "add", "box", "--host", "h", "--path", "/p"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    let output = aghist()
        .args(["sources", "add", "box", "--host", "h2", "--path", "/p2"])
        .env("AGHIST_CONFIG", &config_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "duplicate-source");
}
#[test]
fn sources_remove_unknown_name_fails() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let output = aghist()
        .args(["sources", "remove", "nonexistent"])
        .env("AGHIST_CONFIG", &config_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "source-not-found");
}
#[test]
fn sources_remove_existing_drops_it() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    aghist()
        .args(["sources", "add", "keep", "--host", "h", "--path", "/p1"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();
    aghist()
        .args(["sources", "add", "drop", "--host", "h", "--path", "/p2"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    aghist()
        .args(["sources", "remove", "drop"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    let assert = aghist()
        .args(["sources", "list"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let arr = parsed["sources"].as_array().expect("sources array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "keep");
}
#[test]
fn sources_add_default_transport_is_ssh() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    aghist()
        .args(["sources", "add", "box", "--host", "h", "--path", "/p"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    let assert = aghist()
        .args(["sources", "list"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["sources"][0]["transport"], "ssh");
}
#[test]
fn sources_add_invalid_transport_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let output = aghist()
        .args([
            "sources",
            "add",
            "box",
            "--host",
            "h",
            "--path",
            "/p",
            "--transport",
            "smb",
        ])
        .env("AGHIST_CONFIG", &config_path)
        .output()
        .unwrap();
    // Clap rejects invalid value_parser results with exit 2.
    assert_eq!(output.status.code(), Some(2));
}
#[test]
fn sources_add_rejects_path_like_name() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let output = aghist()
        .args(["sources", "add", "../escape", "--host", "h", "--path", "/p"])
        .env("AGHIST_CONFIG", &config_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
    assert!(
        !config_path.exists(),
        "invalid source must not be persisted"
    );
}
#[test]
fn sources_add_rejects_rsync_option_like_host() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let output = aghist()
        .args(["sources", "add", "box", "--host=-server", "--path", "/p"])
        .env("AGHIST_CONFIG", &config_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
}
/// Writes an executable shell script that imitates rsync: it parses the last
/// arg as the dest dir, creates it, and drops one stub file. Used to drive
/// `aghist sources pull` in tests without a real SSH endpoint.
#[cfg(unix)]
fn write_fake_rsync(dir: &std::path::Path, args_log: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("fake-rsync.sh");
    // The dest dir is always the last arg (rsync convention); drop a stub
    // file there so the post-pull manifest has nonzero file/byte counts.
    // Args are appended (one per line) to args_log so tests can assert on
    // the command rsync was invoked with.
    let body = format!(
        "#!/bin/sh\n\
         for a in \"$@\"; do printf '%s\\n' \"$a\" >> {log:?}; done\n\
         dest=\n\
         for a in \"$@\"; do dest=\"$a\"; done\n\
         mkdir -p \"$dest\"\n\
         printf 'stub-jsonl' > \"$dest/sample.jsonl\"\n\
         exit 0\n",
        log = args_log.display().to_string(),
    );
    std::fs::write(&script, body).unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    script
}
/// Same as `write_fake_rsync` but exits non-zero so we can test error paths.
#[cfg(unix)]
fn write_failing_rsync(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("fake-rsync-fail.sh");
    std::fs::write(&script, "#!/bin/sh\necho 'fake rsync error' >&2\nexit 23\n").unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    script
}
#[cfg(unix)]
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
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "source-not-found");
}
#[cfg(unix)]
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
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
}
#[cfg(unix)]
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
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "source-not-found");
}
#[cfg(unix)]
#[test]
fn sources_pull_invokes_rsync_and_writes_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_dir = dir.path().join("cache");
    let args_log = dir.path().join("rsync-args.txt");
    let fake = write_fake_rsync(dir.path(), &args_log);

    aghist()
        .args([
            "sources",
            "add",
            "laptop",
            "--host",
            "user@laptop.local",
            "--path",
            "/home/user/.claude",
            "--transport",
            "ssh",
        ])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    let assert = aghist()
        .args(["sources", "pull", "laptop"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_RSYNC_BIN", &fake)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let results = parsed["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["name"], "laptop");
    assert_eq!(results[0]["dry_run"], false);
    assert_eq!(results[0]["file_count"], 1);
    assert!(
        results[0]["byte_count"].as_u64().unwrap() > 0,
        "byte_count should be nonzero after pull: {results:?}"
    );

    // The fake rsync logged its args; verify rsync was invoked with the
    // expected source URL, dest dir, and SSH wrapper.
    let logged = std::fs::read_to_string(&args_log).unwrap();
    assert!(
        logged.contains("user@laptop.local:/home/user/.claude/"),
        "rsync source URL missing from args: {logged}"
    );
    assert!(
        logged.contains("ssh -o BatchMode=yes"),
        "ssh wrapper missing from args: {logged}"
    );
    assert!(
        logged.contains("--delete"),
        "missing --delete flag: {logged}"
    );

    // Manifest exists and is parseable.
    let manifest_path = cache_dir.join("laptop").join(".aghist-source.json");
    let manifest_text = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    assert_eq!(manifest["name"], "laptop");
    assert_eq!(manifest["host"], "user@laptop.local");
    assert_eq!(manifest["transport"], "ssh");
    assert_eq!(manifest["last_pull_dry_run"], false);
    assert_eq!(manifest["file_count"], 1);

    // The stub file actually landed under <cache>/laptop/data/.
    let stub = cache_dir.join("laptop").join("data").join("sample.jsonl");
    assert!(stub.exists(), "expected stub file at {}", stub.display());
}
#[cfg(unix)]
#[test]
fn sources_pull_dry_run_passes_flag_and_skips_count() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_dir = dir.path().join("cache");
    let args_log = dir.path().join("rsync-args.txt");
    let fake = write_fake_rsync(dir.path(), &args_log);

    aghist()
        .args(["sources", "add", "box", "--host", "h", "--path", "/p"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    let assert = aghist()
        .args(["sources", "pull", "box", "--dry-run"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_RSYNC_BIN", &fake)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["results"][0]["dry_run"], true);
    // Dry runs report 0/0 — we don't trust the on-disk count after a
    // theoretically-no-op rsync (the fake rsync ignores --dry-run).
    assert_eq!(parsed["results"][0]["file_count"], 0);
    assert_eq!(parsed["results"][0]["byte_count"], 0);

    let logged = std::fs::read_to_string(&args_log).unwrap();
    assert!(
        logged.lines().any(|l| l == "--dry-run"),
        "rsync should have received --dry-run: {logged}"
    );
}
#[cfg(unix)]
#[test]
fn sources_pull_rsync_failure_surfaces_error_kind() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_dir = dir.path().join("cache");
    let fake = write_failing_rsync(dir.path());

    aghist()
        .args(["sources", "add", "box", "--host", "h", "--path", "/p"])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    let output = aghist()
        .args(["sources", "pull", "box"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_RSYNC_BIN", &fake)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "rsync-failed");

    // No manifest should be written when rsync fails.
    let manifest_path = cache_dir.join("box").join(".aghist-source.json");
    assert!(
        !manifest_path.exists(),
        "manifest should not exist after failed pull"
    );
}

#[cfg(unix)]
#[test]
fn sources_pull_refuses_symlinked_cache_root_before_rsync() {
    use std::os::unix::fs::symlink;

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
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "unsafe-cache-dir");
    assert!(
        !args_log.exists(),
        "rsync must not run when cache root is a symlink"
    );
    assert!(
        !escape_dir.join("box").exists(),
        "pull must not write through symlinked cache root"
    );
}

#[cfg(unix)]
#[test]
fn sources_pull_refuses_symlinked_source_cache_dir_before_rsync() {
    use std::os::unix::fs::symlink;

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
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "unsafe-cache-dir");
    assert!(
        !args_log.exists(),
        "rsync must not run when cache root is a symlink"
    );
    assert!(
        !escape_dir.join("sample.jsonl").exists(),
        "pull must not write through symlinked cache root"
    );
}

#[cfg(unix)]
#[test]
fn sources_pull_refuses_symlinked_data_dir_before_rsync() {
    use std::os::unix::fs::symlink;

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
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "unsafe-cache-dir");
    assert!(
        !args_log.exists(),
        "rsync must not run when data dir is a symlink"
    );
    assert!(
        !escape_dir.join("sample.jsonl").exists(),
        "pull must not write through symlinked data dir"
    );
}

#[cfg(unix)]
#[test]
fn sources_pull_rsync_transport_uses_rsync_url_not_ssh() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_dir = dir.path().join("cache");
    let args_log = dir.path().join("rsync-args.txt");
    let fake = write_fake_rsync(dir.path(), &args_log);

    aghist()
        .args([
            "sources",
            "add",
            "daemon",
            "--host",
            "rsync.example.com",
            "--path",
            "/module/path",
            "--transport",
            "rsync",
        ])
        .env("AGHIST_CONFIG", &config_path)
        .assert()
        .success();

    aghist()
        .args(["sources", "pull", "daemon"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_RSYNC_BIN", &fake)
        .assert()
        .success();

    let logged = std::fs::read_to_string(&args_log).unwrap();
    assert!(
        logged.contains("rsync://rsync.example.com/module/path/"),
        "expected rsync:// URL: {logged}"
    );
    // SSH wrapper must NOT be added for rsync-daemon transport.
    assert!(
        !logged.contains("BatchMode=yes"),
        "ssh wrapper should not appear for rsync transport: {logged}"
    );
}
#[cfg(unix)]
#[test]
fn sources_pull_all_iterates_every_source() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cache_dir = dir.path().join("cache");
    let args_log = dir.path().join("rsync-args.txt");
    let fake = write_fake_rsync(dir.path(), &args_log);

    for (name, host) in [("a", "h1"), ("b", "h2")] {
        aghist()
            .args(["sources", "add", name, "--host", host, "--path", "/p"])
            .env("AGHIST_CONFIG", &config_path)
            .assert()
            .success();
    }

    let assert = aghist()
        .args(["sources", "pull", "--all"])
        .env("AGHIST_CONFIG", &config_path)
        .env("AGHIST_SOURCES_CACHE_DIR", &cache_dir)
        .env("AGHIST_RSYNC_BIN", &fake)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let results = parsed["results"].as_array().expect("results array");
    assert_eq!(results.len(), 2);
    let names: Vec<&str> = results
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"a"), "{names:?}");
    assert!(names.contains(&"b"), "{names:?}");

    // Each source got its own manifest.
    assert!(cache_dir.join("a").join(".aghist-source.json").exists());
    assert!(cache_dir.join("b").join(".aghist-source.json").exists());
}
