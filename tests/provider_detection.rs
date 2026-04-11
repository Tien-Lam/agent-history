mod common;

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

use aghist::config::Config;
use aghist::model::Provider;
use aghist::provider::claude_code::ClaudeCodeProvider;
use aghist::provider::codex_cli::CodexCliProvider;
use aghist::provider::copilot_cli::CopilotCliProvider;
use aghist::provider::gemini_cli::GeminiCliProvider;
use aghist::provider::opencode::OpenCodeProvider;
use aghist::provider::HistoryProvider;

fn aghist() -> Command {
    Command::cargo_bin("aghist").unwrap()
}

// ─── detect_all_providers via CLI (subprocess, safe AGHIST_HOME override) ──

#[test]
fn empty_home_detects_nothing() {
    let dir = tempfile::tempdir().unwrap();
    aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Total: 0 sessions"));
}

#[test]
fn detects_claude_when_dir_exists() {
    let dir = tempfile::tempdir().unwrap();
    let claude_dir = dir.path().join(".claude");
    fs::create_dir_all(claude_dir.join("projects")).unwrap();
    fs::write(claude_dir.join("history.jsonl"), "").unwrap();

    aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude Code: 0 sessions"));
}

#[test]
fn detects_copilot_when_dir_exists() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".copilot").join("session-state")).unwrap();

    aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Copilot CLI: 0 sessions"));
}

#[test]
fn detects_gemini_when_dir_exists() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".gemini")).unwrap();

    aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Gemini CLI: 0 sessions"));
}

#[test]
fn detects_codex_when_dir_exists() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".codex").join("sessions")).unwrap();

    aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Codex CLI: 0 sessions"));
}

#[test]
fn detects_multiple_providers() {
    let dir = tempfile::tempdir().unwrap();
    let claude_dir = dir.path().join(".claude");
    fs::create_dir_all(claude_dir.join("projects")).unwrap();
    fs::write(claude_dir.join("history.jsonl"), "").unwrap();
    fs::create_dir_all(dir.path().join(".gemini")).unwrap();
    fs::create_dir_all(dir.path().join(".codex").join("sessions")).unwrap();

    aghist()
        .arg("--list")
        .env("AGHIST_HOME", dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude Code"))
        .stdout(predicate::str::contains("Gemini CLI"))
        .stdout(predicate::str::contains("Codex CLI"));
}

// ─── Provider constructor tests (direct, no env mutation needed) ───────────

#[test]
fn claude_provider_with_nonexistent_dir() {
    let dir = tempfile::tempdir().unwrap();
    let provider = ClaudeCodeProvider::new(vec![dir.path().join("does-not-exist")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn copilot_provider_with_nonexistent_dir() {
    let dir = tempfile::tempdir().unwrap();
    let provider = CopilotCliProvider::new(vec![dir.path().join("does-not-exist")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn gemini_provider_with_nonexistent_dir() {
    let dir = tempfile::tempdir().unwrap();
    let provider = GeminiCliProvider::new(vec![dir.path().join("does-not-exist")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn codex_provider_with_nonexistent_dir() {
    let dir = tempfile::tempdir().unwrap();
    let provider = CodexCliProvider::new(vec![dir.path().join("does-not-exist")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn opencode_provider_with_nonexistent_dir() {
    let dir = tempfile::tempdir().unwrap();
    let provider = OpenCodeProvider::new(vec![dir.path().join("does-not-exist")]);
    let sessions = provider.discover_sessions().unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn generated_claude_fixtures_discoverable() {
    let fixture = common::fixtures::claude_single_session(4);
    let provider = ClaudeCodeProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].message_count, 4);
    assert_eq!(sessions[0].provider, Provider::ClaudeCode);
}

#[test]
fn generated_copilot_fixtures_discoverable() {
    let fixture = common::fixtures::copilot_single_session(4);
    let provider = CopilotCliProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].provider, Provider::CopilotCli);
}

#[test]
fn generated_gemini_fixtures_discoverable() {
    let fixture = common::fixtures::gemini_single_session(4);
    let provider = GeminiCliProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].provider, Provider::GeminiCli);
}

#[test]
fn generated_codex_fixtures_discoverable() {
    let fixture = common::fixtures::codex_single_session(4);
    let provider = CodexCliProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].provider, Provider::CodexCli);
}

#[test]
fn generated_opencode_fixtures_discoverable() {
    let fixture = common::fixtures::opencode_single_session(4);
    let provider = OpenCodeProvider::new(vec![fixture.base_path.clone()]);
    let sessions = provider.discover_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].provider, Provider::OpenCode);
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

// ─── Multi-provider fixture generation ─────────────────────────────────────

#[test]
fn all_generated_providers_discoverable() {
    let (dirs, providers) = common::fixtures::all_generated_providers(2, 4);

    let mut total = 0;
    for p in &providers {
        let sessions = p.discover_sessions().unwrap();
        assert!(
            !sessions.is_empty(),
            "{:?} provider should have sessions",
            p.provider()
        );
        total += sessions.len();
    }
    drop(dirs);

    // 5 providers * 2 sessions each
    assert_eq!(total, 10);
}

#[test]
fn all_generated_providers_messages_loadable() {
    let (dirs, providers) = common::fixtures::all_generated_providers(1, 6);

    for p in &providers {
        let sessions = p.discover_sessions().unwrap();
        for session in &sessions {
            let messages = p.load_messages(session).unwrap();
            assert!(
                !messages.is_empty(),
                "{:?} session should have messages",
                p.provider()
            );
        }
    }
    drop(dirs);
}
