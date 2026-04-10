use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::Provider;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub cache_size: usize,
    pub show_tool_calls: bool,
    pub max_messages_per_session: usize,
    pub providers: ProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub enabled: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cache_size: 20,
            show_tool_calls: false,
            max_messages_per_session: 5000,
            providers: ProviderConfig::default(),
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            enabled: vec![
                "claude-code".into(),
                "copilot-cli".into(),
                "gemini-cli".into(),
                "codex-cli".into(),
                "opencode".into(),
            ],
        }
    }
}

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "aghist")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };

        let mut config: Self = match std::fs::read_to_string(&path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        };
        if config.cache_size == 0 {
            config.cache_size = 1;
        }
        config
    }

    pub fn enabled_providers(&self) -> HashSet<Provider> {
        self.providers
            .enabled
            .iter()
            .filter_map(|s| match s.as_str() {
                "claude-code" => Some(Provider::ClaudeCode),
                "copilot-cli" => Some(Provider::CopilotCli),
                "gemini-cli" => Some(Provider::GeminiCli),
                "codex-cli" => Some(Provider::CodexCli),
                "opencode" => Some(Provider::OpenCode),
                _ => None,
            })
            .collect()
    }
}
