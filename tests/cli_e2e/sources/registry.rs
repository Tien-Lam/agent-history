use super::super::aghist;

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
fn sources_remove_rejects_invalid_name() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let output = aghist()
        .args(["sources", "remove", "../escape"])
        .env("AGHIST_CONFIG", &config_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
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
    assert_eq!(output.status.code(), Some(2));
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
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let line = stderr
        .lines()
        .find(|l| l.starts_with('{'))
        .expect("expected JSON error envelope");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(parsed["error"]["kind"], "usage");
}
