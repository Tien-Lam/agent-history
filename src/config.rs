use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::fs_atomic;
use crate::model::Provider;

mod sources;

pub use sources::{
    sources_cache_root, validate_rsync_endpoint, validate_source_name, RemoteSource,
    SourceCacheManifest, Transport,
};

#[derive(Debug, Error)]
pub enum ConfigLoadError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub cache_size: usize,
    pub show_tool_calls: bool,
    pub max_messages_per_session: usize,
    pub providers: ProviderConfig,
    /// Registered remote sources (e.g. SSH/rsync targets on other hosts).
    /// Local provider directories are still auto-detected; this list is for
    /// hosts whose history dirs aghist can't see directly.
    #[serde(rename = "sources", skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<RemoteSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub enabled: Vec<String>,
    /// Per-provider allowlist for the `aghist mcp` server. When `None`, all
    /// `enabled` providers are visible to MCP clients. When `Some`, only the
    /// intersection of `mcp_exposed` and `enabled` is exposed — letting users
    /// hide history (e.g. a personal Claude account) from agents that don't
    /// need it without disabling the provider for the local TUI/CLI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_exposed: Option<Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cache_size: 20,
            show_tool_calls: false,
            max_messages_per_session: 5000,
            providers: ProviderConfig::default(),
            sources: Vec::new(),
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            enabled: Provider::all()
                .iter()
                .map(|p| p.slug().to_string())
                .collect(),
            mcp_exposed: None,
        }
    }
}

impl Config {
    /// Default config file location, e.g. `~/.config/aghist/config.toml` on Linux.
    pub fn config_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "aghist")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    /// Resolved config path: respects `AGHIST_CONFIG` (file path) when set,
    /// otherwise falls back to `config_path()`. Tests use the env var to
    /// avoid touching the user's real config.
    pub fn resolved_path() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("AGHIST_CONFIG") {
            if !p.is_empty() {
                return Some(PathBuf::from(p));
            }
        }
        Self::config_path()
    }

    pub fn load() -> Self {
        let Some(path) = Self::resolved_path() else {
            return Self::default();
        };
        Self::load_from(&path)
    }

    pub fn try_load() -> Result<Self, ConfigLoadError> {
        let Some(path) = Self::resolved_path() else {
            return Ok(Self::default());
        };
        Self::try_load_from(&path)
    }

    pub fn try_load_from(path: &Path) -> Result<Self, ConfigLoadError> {
        let contents = match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(ConfigLoadError::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let mut config: Self =
            toml::from_str(&contents).map_err(|source| ConfigLoadError::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?;
        config.normalize();
        Ok(config)
    }

    pub fn load_from(path: &Path) -> Self {
        let mut config: Self = match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "warning: failed to parse {}: {e}; using defaults",
                        path.display()
                    );
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        };
        config.normalize();
        config
    }

    fn normalize(&mut self) {
        if self.cache_size == 0 {
            self.cache_size = 1;
        }
    }

    /// Serialize to TOML and write atomically to `path`. Creates the parent
    /// directory if missing.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let toml = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs_atomic::write(path, toml.as_bytes())?;
        Ok(())
    }

    pub fn enabled_providers(&self) -> HashSet<Provider> {
        self.providers
            .enabled
            .iter()
            .filter_map(|s| Provider::from_slug(s))
            .collect()
    }

    /// Providers that may be exposed by the `aghist mcp` server.
    ///
    /// Resolution rules:
    /// - When `providers.mcp_exposed` is unset, fall back to `enabled_providers()`.
    /// - When set, return the intersection with `enabled_providers()`. Slugs
    ///   that aren't in `enabled` (or aren't valid providers) are silently
    ///   dropped — narrowing only, no escalation.
    pub fn mcp_exposed_providers(&self) -> HashSet<Provider> {
        let enabled = self.enabled_providers();
        let Some(allow) = self.providers.mcp_exposed.as_ref() else {
            return enabled;
        };
        allow
            .iter()
            .filter_map(|s| Provider::from_slug(s))
            .filter(|p| enabled.contains(p))
            .collect()
    }
}
