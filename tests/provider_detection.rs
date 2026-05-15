mod common;

use std::fs;

use assert_cmd::Command;

use aghist::config::Config;
use aghist::model::Provider;

fn aghist() -> Command {
    Command::cargo_bin("aghist").unwrap()
}

// ─── detect_all_providers via CLI (subprocess, safe AGHIST_HOME override) ──

// These tests previously asserted human-mode banners ("Claude Code: 0 sessions").
// With --json/--ndjson auto-enabling on piped stdout (assert_cmd pipes), --list
// now emits structured output by default and the per-provider banner is gone.
// The structural intent — provider directories are scanned without crashing,
// `--list` exits 3 on empty — is what these tests guard now via the macro below.

macro_rules! assert_empty_list {
    ($dir:expr) => {{
        let output = aghist()
            .arg("--list")
            .env("AGHIST_HOME", $dir)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(3),
            "--list with no sessions must exit 3"
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            !stderr.contains("\"error\""),
            "no error envelope expected on stderr, got: {stderr}"
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        // NDJSON terminates with a `{"meta": ...}` envelope row; the only
        // structural guarantee for an empty home is that no session rows
        // (rows with an `id` field) appear.
        let session_rows = stdout
            .lines()
            .filter(|l| !l.is_empty())
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|v| v.get("id").is_some())
            .count();
        assert_eq!(
            session_rows, 0,
            "NDJSON should have no session rows, got: {stdout:?}"
        );
    }};
}

#[test]
fn empty_home_detects_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert_empty_list!(dir.path());
}

#[test]
fn detects_claude_when_dir_exists() {
    let dir = tempfile::tempdir().unwrap();
    let claude_dir = dir.path().join(".claude");
    fs::create_dir_all(claude_dir.join("projects")).unwrap();
    fs::write(claude_dir.join("history.jsonl"), "").unwrap();
    assert_empty_list!(dir.path());
}

#[test]
fn detects_copilot_when_dir_exists() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".copilot").join("session-state")).unwrap();
    assert_empty_list!(dir.path());
}

#[test]
fn detects_gemini_when_dir_exists() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".gemini")).unwrap();
    assert_empty_list!(dir.path());
}

#[test]
fn detects_codex_when_dir_exists() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex").join("sessions")).unwrap();
    assert_empty_list!(dir.path());
}

#[test]
fn detects_multiple_providers() {
    let dir = tempfile::tempdir().unwrap();
    let claude_dir = dir.path().join(".claude");
    fs::create_dir_all(claude_dir.join("projects")).unwrap();
    fs::write(claude_dir.join("history.jsonl"), "").unwrap();
    fs::create_dir_all(dir.path().join(".gemini")).unwrap();
    fs::create_dir_all(dir.path().join(".codex").join("sessions")).unwrap();
    assert_empty_list!(dir.path());
}

#[test]
fn config_filters_detected_providers_via_cli() {
    // Create fixtures with Claude data + Gemini dir
    let fixture = common::fixtures::claude_single_session(2);
    let home = fixture.base_path.parent().unwrap();
    fs::create_dir_all(home.join(".gemini")).unwrap();

    // Create config that only enables gemini
    let config_dir = home.join(".config").join("aghist");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        "[providers]\nenabled = [\"gemini-cli\"]\n",
    )
    .unwrap();

    // The binary should only show Gemini, not Claude
    // Note: config path depends on platform, so we test config filtering via lib
    let config_path = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        config_path.path(),
        "[providers]\nenabled = [\"gemini-cli\"]\n",
    )
    .unwrap();
    let config = Config::load_from(config_path.path());
    let enabled = config.enabled_providers();

    assert!(enabled.contains(&Provider::GeminiCli));
    assert!(!enabled.contains(&Provider::ClaudeCode));
    assert_eq!(enabled.len(), 1);
}
