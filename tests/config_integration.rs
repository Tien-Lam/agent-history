mod common;

use std::fs;

use aghist::config::Config;
use aghist::model::Provider;

#[test]
fn default_config_when_file_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nonexistent.toml");
    let config = Config::load_from(&path);

    assert_eq!(config.cache_size, 20);
    assert!(!config.show_tool_calls);
    assert_eq!(config.max_messages_per_session, 5000);
    assert_eq!(config.providers.enabled.len(), 5);
}

#[test]
fn custom_cache_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "cache_size = 50\n").unwrap();
    let config = Config::load_from(&path);

    assert_eq!(config.cache_size, 50);
    // Other fields should be defaults
    assert!(!config.show_tool_calls);
    assert_eq!(config.max_messages_per_session, 5000);
}

#[test]
fn cache_size_zero_clamped_to_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "cache_size = 0\n").unwrap();
    let config = Config::load_from(&path);

    assert_eq!(config.cache_size, 1);
}

#[test]
fn show_tool_calls_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "show_tool_calls = true\n").unwrap();
    let config = Config::load_from(&path);

    assert!(config.show_tool_calls);
}

#[test]
fn custom_enabled_providers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "[providers]\nenabled = [\"claude-code\", \"gemini-cli\"]\n",
    )
    .unwrap();
    let config = Config::load_from(&path);

    let enabled = config.enabled_providers();
    assert_eq!(enabled.len(), 2);
    assert!(enabled.contains(&Provider::ClaudeCode));
    assert!(enabled.contains(&Provider::GeminiCli));
    assert!(!enabled.contains(&Provider::CopilotCli));
}

#[test]
fn corrupt_toml_falls_back_to_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "this is [[[not valid toml!!!").unwrap();
    let config = Config::load_from(&path);

    assert_eq!(config.cache_size, 20);
    assert_eq!(config.providers.enabled.len(), 5);
}

#[test]
fn partial_config_fills_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "max_messages_per_session = 100\n").unwrap();
    let config = Config::load_from(&path);

    assert_eq!(config.max_messages_per_session, 100);
    assert_eq!(config.cache_size, 20);
    assert!(!config.show_tool_calls);
    assert_eq!(config.providers.enabled.len(), 5);
}

#[test]
fn unknown_provider_names_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "[providers]\nenabled = [\"claude-code\", \"nonexistent-provider\"]\n",
    )
    .unwrap();
    let config = Config::load_from(&path);

    let enabled = config.enabled_providers();
    assert_eq!(enabled.len(), 1);
    assert!(enabled.contains(&Provider::ClaudeCode));
}

#[test]
fn empty_enabled_providers_disables_all() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[providers]\nenabled = []\n").unwrap();
    let config = Config::load_from(&path);

    let enabled = config.enabled_providers();
    assert!(enabled.is_empty());
}
