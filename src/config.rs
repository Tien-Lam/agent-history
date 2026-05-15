use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::Provider;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSource {
    pub name: String,
    pub host: String,
    pub path: String,
    #[serde(default)]
    pub transport: Transport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    #[default]
    Ssh,
    Rsync,
}

impl Transport {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Ssh => "ssh",
            Self::Rsync => "rsync",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "ssh" => Some(Self::Ssh),
            "rsync" => Some(Self::Rsync),
            _ => None,
        }
    }
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
    /// directory if missing. Returns the path that was written.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let toml = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        // Write to a sibling temp file, then rename — avoids partial writes.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, toml)?;
        std::fs::rename(&tmp, path)?;
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

impl RemoteSource {
    pub fn validate(&self) -> Result<(), String> {
        validate_source_name(&self.name)?;
        validate_rsync_endpoint(&self.host, "--host")?;
        validate_rsync_endpoint(&self.path, "--path")?;
        Ok(())
    }

    /// Cache directory for this source, e.g. `<root>/<name>/`.
    pub fn cache_dir(&self, root: &Path) -> PathBuf {
        root.join(&self.name)
    }

    /// Where rsync mirrors the remote tree to. Files under this dir mirror
    /// `<host>:<path>/`.
    pub fn data_dir(&self, root: &Path) -> PathBuf {
        self.cache_dir(root).join("data")
    }

    /// Path to the per-source manifest written by `aghist sources pull`.
    pub fn manifest_path(&self, root: &Path) -> PathBuf {
        self.cache_dir(root).join(".aghist-source.json")
    }
}

pub fn validate_source_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("source name must not be empty".to_string());
    }
    if trimmed != name {
        return Err("source name must not contain leading or trailing whitespace".to_string());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("source name must not be '.' or '..'".to_string());
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err("source name must not be empty".to_string());
    };
    if !first.is_ascii_alphanumeric() {
        return Err("source name must start with an ASCII letter or digit".to_string());
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("source name may contain only ASCII letters, digits, '-' and '_'".to_string());
    }
    Ok(())
}

pub fn validate_rsync_endpoint(value: &str, label: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if trimmed != value {
        return Err(format!(
            "{label} must not contain leading or trailing whitespace"
        ));
    }
    if trimmed.starts_with('-') {
        return Err(format!("{label} must not start with '-'"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(format!("{label} must not contain control characters"));
    }
    Ok(())
}

/// Default sources cache root, e.g. `<aghist cache_dir>/sources/`. Respects
/// `AGHIST_SOURCES_CACHE_DIR` for tests. Returns `None` when no home/XDG dirs
/// exist and the env var is unset.
pub fn sources_cache_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AGHIST_SOURCES_CACHE_DIR") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    directories::ProjectDirs::from("", "", "aghist").map(|dirs| dirs.cache_dir().join("sources"))
}

/// Manifest written under `<cache>/<name>/.aghist-source.json` after each
/// `aghist sources pull`. Captures a snapshot of the source config at pull
/// time plus byte/file accounting from the local mirror.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCacheManifest {
    pub name: String,
    pub host: String,
    pub path: String,
    pub transport: Transport,
    pub data_dir: String,
    pub last_pulled_at: DateTime<Utc>,
    #[serde(default)]
    pub last_pull_dry_run: bool,
    pub byte_count: u64,
    pub file_count: u64,
}

impl SourceCacheManifest {
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}
