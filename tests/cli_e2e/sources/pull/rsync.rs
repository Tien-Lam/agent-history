use super::*;

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
    assert_eq!(parsed["summary"]["source_count"], 1);
    assert_eq!(parsed["summary"]["file_count"], 1);
    assert_eq!(parsed["summary"]["dry_run"], false);
    assert!(
        parsed["summary"]["byte_count"].as_u64().unwrap() > 0,
        "summary byte_count should be nonzero after pull: {parsed:?}"
    );

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

    let manifest_path = cache_dir.join("laptop").join(".aghist-source.json");
    let manifest_text = std::fs::read_to_string(&manifest_path).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    assert_eq!(manifest["name"], "laptop");
    assert_eq!(manifest["host"], "user@laptop.local");
    assert_eq!(manifest["transport"], "ssh");
    assert_eq!(manifest["last_pull_dry_run"], false);
    assert_eq!(manifest["file_count"], 1);

    let stub = cache_dir.join("laptop").join("data").join("sample.jsonl");
    assert!(stub.exists(), "expected stub file at {}", stub.display());
}

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
    assert_eq!(parsed["results"][0]["file_count"], 0);
    assert_eq!(parsed["results"][0]["byte_count"], 0);
    assert_eq!(parsed["summary"]["source_count"], 1);
    assert_eq!(parsed["summary"]["file_count"], 0);
    assert_eq!(parsed["summary"]["byte_count"], 0);
    assert_eq!(parsed["summary"]["dry_run"], true);

    let logged = std::fs::read_to_string(&args_log).unwrap();
    assert!(
        logged.lines().any(|line| line == "--dry-run"),
        "rsync should have received --dry-run: {logged}"
    );
    let destination = logged.lines().last().expect("rsync destination arg");
    assert!(
        !destination.starts_with(cache_dir.to_str().expect("utf8 cache dir")),
        "dry-run should use a temporary destination, got {destination}"
    );
    assert!(
        !cache_dir.join("box").exists(),
        "dry-run must not create cache source dirs"
    );
    assert!(
        !cache_dir.join("box").join(".aghist-source.json").exists(),
        "dry-run must not write a source manifest"
    );
}

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
    assert_eq!(stderr_error_kind(&output), "rsync-failed");

    let manifest_path = cache_dir.join("box").join(".aghist-source.json");
    assert!(
        !manifest_path.exists(),
        "manifest should not exist after failed pull"
    );
}

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
    assert!(
        !logged.contains("BatchMode=yes"),
        "ssh wrapper should not appear for rsync transport: {logged}"
    );
}

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
        .map(|result| result["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"a"), "{names:?}");
    assert!(names.contains(&"b"), "{names:?}");
    assert_eq!(parsed["summary"]["source_count"], 2);
    assert_eq!(parsed["summary"]["file_count"], 2);
    assert_eq!(parsed["summary"]["dry_run"], false);
    assert!(
        parsed["summary"]["byte_count"].as_u64().unwrap() > 0,
        "summary byte_count should include pulled sources: {parsed:?}"
    );

    assert!(cache_dir.join("a").join(".aghist-source.json").exists());
    assert!(cache_dir.join("b").join(".aghist-source.json").exists());
}
