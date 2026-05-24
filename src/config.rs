use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::fs_atomic;
use crate::fs_read;
use crate::model::Provider;

mod sources;

const MAX_CONFIG_BYTES: usize = 1024 * 1024;

pub use sources::{
    sources_cache_root, validate_rsync_endpoint, validate_rsync_host, validate_rsync_path,
    validate_source_name, RemoteSource, SourceCacheManifest, SourceCacheManifestLoadError,
    Transport, MAX_REMOTE_SOURCES, MAX_SOURCE_NAME_BYTES,
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
    #[error("unknown provider slug '{slug}' in {field} of {path}; expected one of: {expected}")]
    UnknownProviderSlug {
        path: PathBuf,
        field: &'static str,
        slug: String,
        expected: String,
    },
    #[error("invalid remote source #{index} ('{name}') in {path}: {reason}")]
    InvalidRemoteSource {
        path: PathBuf,
        index: usize,
        name: String,
        reason: String,
    },
    #[error("too many remote sources in {path}: {count} exceeds {max}")]
    TooManyRemoteSources {
        path: PathBuf,
        count: usize,
        max: usize,
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
        if let Some(path) = config_path_from_env_value(std::env::var("AGHIST_CONFIG").ok()) {
            return Some(path);
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
        let contents = match fs_read::read_to_string_limited(path, MAX_CONFIG_BYTES) {
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
        config.validate_provider_slugs(path)?;
        config.validate_remote_sources(path)?;
        Ok(config)
    }

    pub fn load_from(path: &Path) -> Self {
        match Self::try_load_from(path) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("warning: failed to load config: {error}; using defaults");
                Self::default()
            }
        }
    }

    fn normalize(&mut self) {
        if self.cache_size == 0 {
            self.cache_size = 1;
        }
    }

    fn validate_provider_slugs(&self, path: &Path) -> Result<(), ConfigLoadError> {
        validate_provider_slug_list(path, "providers.enabled", &self.providers.enabled)?;
        if let Some(exposed) = self.providers.mcp_exposed.as_ref() {
            validate_provider_slug_list(path, "providers.mcp_exposed", exposed)?;
        }
        Ok(())
    }

    fn validate_remote_sources(&self, path: &Path) -> Result<(), ConfigLoadError> {
        if self.sources.len() > MAX_REMOTE_SOURCES {
            return Err(ConfigLoadError::TooManyRemoteSources {
                path: path.to_path_buf(),
                count: self.sources.len(),
                max: MAX_REMOTE_SOURCES,
            });
        }
        let mut seen = HashSet::new();
        for (idx, source) in self.sources.iter().enumerate() {
            source
                .validate()
                .map_err(|reason| invalid_remote_source(path, idx, source, reason))?;
            if !seen.insert(source.name.as_str()) {
                return Err(invalid_remote_source(
                    path,
                    idx,
                    source,
                    "duplicate source name".to_string(),
                ));
            }
        }
        Ok(())
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
    /// - When set, return the intersection with `enabled_providers()`. Valid
    ///   providers that aren't in `enabled` are dropped — narrowing only, no
    ///   escalation.
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

fn validate_provider_slug_list(
    path: &Path,
    field: &'static str,
    slugs: &[String],
) -> Result<(), ConfigLoadError> {
    if let Some(slug) = slugs
        .iter()
        .find(|slug| Provider::from_slug(slug.as_str()).is_none())
    {
        return Err(ConfigLoadError::UnknownProviderSlug {
            path: path.to_path_buf(),
            field,
            slug: slug.clone(),
            expected: expected_provider_slugs(),
        });
    }
    Ok(())
}

fn expected_provider_slugs() -> String {
    Provider::all()
        .iter()
        .map(|provider| provider.slug())
        .collect::<Vec<_>>()
        .join(", ")
}

fn invalid_remote_source(
    path: &Path,
    idx: usize,
    source: &RemoteSource,
    reason: String,
) -> ConfigLoadError {
    ConfigLoadError::InvalidRemoteSource {
        path: path.to_path_buf(),
        index: idx + 1,
        name: source.name.clone(),
        reason,
    }
}

fn config_path_from_env_value(value: Option<String>) -> Option<PathBuf> {
    value
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::path::PathBuf;

    use super::{config_path_from_env_value, Config, ConfigLoadError, MAX_REMOTE_SOURCES};

    #[test]
    fn config_path_from_env_value_ignores_blank_values() {
        assert_eq!(config_path_from_env_value(None), None);
        assert_eq!(config_path_from_env_value(Some(String::new())), None);
        assert_eq!(config_path_from_env_value(Some(" \t ".to_string())), None);
        assert_eq!(
            config_path_from_env_value(Some("/tmp/aghist.toml".to_string())),
            Some(PathBuf::from("/tmp/aghist.toml"))
        );
    }

    #[test]
    fn config_load_rejects_invalid_remote_source_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[[sources]]
name = "../outside"
host = "example.test"
path = "~/.claude"
"#,
        )
        .unwrap();

        let err = Config::try_load_from(&path).unwrap_err();

        assert!(matches!(
            err,
            ConfigLoadError::InvalidRemoteSource {
                index: 1,
                reason,
                ..
            } if reason.contains("source name")
        ));
    }

    #[test]
    fn config_load_rejects_duplicate_remote_source_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[[sources]]
name = "work"
host = "one.example"
path = "~/.claude"

[[sources]]
name = "work"
host = "two.example"
path = "~/.claude"
"#,
        )
        .unwrap();

        let err = Config::try_load_from(&path).unwrap_err();

        assert!(matches!(
            err,
            ConfigLoadError::InvalidRemoteSource {
                index: 2,
                reason,
                ..
            } if reason == "duplicate source name"
        ));
    }

    #[test]
    fn config_load_rejects_too_many_remote_sources() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut toml = String::new();
        for idx in 0..=MAX_REMOTE_SOURCES {
            write!(
                toml,
                r#"
[[sources]]
name = "source_{idx}"
host = "example{idx}.test"
path = "~/.claude"
"#
            )
            .unwrap();
        }
        std::fs::write(&path, toml).unwrap();

        let err = Config::try_load_from(&path).unwrap_err();

        assert!(matches!(
            err,
            ConfigLoadError::TooManyRemoteSources {
                count,
                max: MAX_REMOTE_SOURCES,
                ..
            } if count == MAX_REMOTE_SOURCES + 1
        ));
    }
}
