use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::Transport;
use crate::fs_atomic;
use crate::fs_read;

const MAX_SOURCE_CACHE_MANIFEST_BYTES: usize = 1024 * 1024;

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
        let text = fs_read::read_to_string_limited(path, MAX_SOURCE_CACHE_MANIFEST_BYTES).map_err(
            |source| SourceCacheManifestLoadError::Read {
                path: path.to_path_buf(),
                source,
            },
        )?;
        serde_json::from_str(&text).map_err(|source| SourceCacheManifestLoadError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn load(path: &Path) -> Option<Self> {
        Self::try_load(path).ok()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs_atomic::write(path, json.as_bytes())?;
        Ok(())
    }
}
