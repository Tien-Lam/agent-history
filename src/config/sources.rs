use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

mod manifest;
mod validation;

pub use manifest::{SourceCacheManifest, SourceCacheManifestLoadError};
pub use validation::{
    validate_rsync_endpoint, validate_rsync_host, validate_rsync_path, validate_source_name,
};

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
        validate_rsync_host(&self.host, "--host")?;
        validate_rsync_path(&self.path, "--path")?;
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
