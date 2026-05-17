mod common;

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use rusqlite::Connection;

use aghist::config::Config;
use aghist::model::Provider;

fn aghist() -> Command {
    common::helpers::aghist_command()
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
            .env("AGHIST_CONFIG", $dir.join("missing-config.toml"))
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

fn detected_providers(home: &Path) -> Vec<String> {
    let index_dir = tempfile::tempdir().unwrap();
    let output = aghist()
        .arg("health")
        .arg("--json")
        .env("AGHIST_HOME", home)
        .env("AGHIST_CONFIG", home.join("missing-config.toml"))
        .env("AGHIST_INDEX_DIR", index_dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(parsed["ok"], true);

    parsed["provider_fidelity"]
        .as_array()
        .expect("provider_fidelity array")
        .iter()
        .map(|row| row["provider"].as_str().expect("provider slug").to_string())
        .collect()
}

fn create_provider_marker(home: &Path, provider: Provider) {
    match provider {
        Provider::ClaudeCode => {
            fs::create_dir_all(home.join(".claude")).unwrap();
        }
        Provider::CopilotCli => {
            fs::create_dir_all(home.join(".copilot").join("session-state")).unwrap();
        }
        Provider::GeminiCli => {
            fs::create_dir_all(home.join(".gemini")).unwrap();
        }
        Provider::CodexCli => {
            fs::create_dir_all(home.join(".codex").join("sessions")).unwrap();
        }
        Provider::OpenCode => {
            fs::create_dir_all(
                home.join(".local")
                    .join("share")
                    .join("opencode")
                    .join("storage"),
            )
            .unwrap();
        }
        Provider::Cursor => {
            let db_path = home
                .join(".config")
                .join("Cursor")
                .join("User")
                .join("globalStorage")
                .join("state.vscdb");
            fs::create_dir_all(db_path.parent().unwrap()).unwrap();
            Connection::open(db_path).unwrap();
        }
        Provider::Aider => {
            fs::create_dir_all(home.join("projects")).unwrap();
        }
        Provider::ZedAi => {
            fs::create_dir_all(
                home.join(".local")
                    .join("share")
                    .join("zed")
                    .join("conversations"),
            )
            .unwrap();
        }
        Provider::Cline => {
            fs::create_dir_all(
                home.join(".config")
                    .join("Code")
                    .join("User")
                    .join("globalStorage")
                    .join("saoudrizwan.claude-dev")
                    .join("tasks"),
            )
            .unwrap();
        }
        Provider::ContinueDev => {
            fs::create_dir_all(home.join(".continue").join("sessions")).unwrap();
        }
    }
}

#[test]
fn empty_home_detects_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert_empty_list!(dir.path());
    assert!(detected_providers(dir.path()).is_empty());
}

#[test]
fn detects_each_provider_when_marker_exists() {
    for provider in Provider::all() {
        let dir = tempfile::tempdir().unwrap();
        create_provider_marker(dir.path(), *provider);
        assert_empty_list!(dir.path());
        assert_eq!(detected_providers(dir.path()), vec![provider.slug()]);
    }
}

#[test]
fn detects_all_provider_markers() {
    let dir = tempfile::tempdir().unwrap();
    for provider in Provider::all() {
        create_provider_marker(dir.path(), *provider);
    }
    assert_empty_list!(dir.path());
    let expected: Vec<String> = Provider::all()
        .iter()
        .map(|provider| provider.slug().to_string())
        .collect();
    assert_eq!(detected_providers(dir.path()), expected);
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
