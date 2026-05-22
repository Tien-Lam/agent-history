use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
    if trimmed == crate::federated::LOCAL_SOURCE {
        return Err("source name 'local' is reserved".to_string());
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

#[derive(Debug, Error)]
pub enum SourceCacheManifestLoadError {
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
        source: serde_json::Error,
    },
}

impl SourceCacheManifest {
    pub fn try_load(path: &Path) -> Result<Self, SourceCacheManifestLoadError> {
        let text =
            std::fs::read_to_string(path).map_err(|source| SourceCacheManifestLoadError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        serde_json::from_str(&text).map_err(|source| SourceCacheManifestLoadError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn load(path: &Path) -> Option<Self> {
        Self::try_load(path).ok()
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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{validate_rsync_endpoint, validate_source_name};

    fn valid_source_name_strategy() -> impl Strategy<Value = String> {
        "[A-Za-z0-9][A-Za-z0-9_-]{0,40}"
            .prop_filter("source name must not be reserved", |name| name != "local")
    }

    proptest! {
        #[test]
        fn valid_source_names_are_accepted(name in valid_source_name_strategy()) {
            prop_assert!(validate_source_name(&name).is_ok());
        }

        #[test]
        fn source_names_with_forbidden_characters_are_rejected(
            prefix in "[A-Za-z0-9][A-Za-z0-9_-]{0,12}",
            suffix in "[A-Za-z0-9_-]{0,12}",
            bad in prop::sample::select(vec!['/', '.', ':', ' ', '\t', '\n', '\\']),
        ) {
            let name = format!("{prefix}{bad}{suffix}");
            prop_assert!(validate_source_name(&name).is_err());
        }

        #[test]
        fn rsync_endpoints_reject_control_characters(
            prefix in "[A-Za-z0-9_./:@-]{1,20}",
            suffix in "[A-Za-z0-9_./:@-]{0,20}",
            control in prop::sample::select(vec!['\0', '\n', '\r', '\t']),
        ) {
            let endpoint = format!("{prefix}{control}{suffix}");
            prop_assert!(validate_rsync_endpoint(&endpoint, "--host").is_err());
        }
    }
}
