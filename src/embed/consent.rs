use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::EmbedError;
use crate::fs_atomic;

const CONSENT_FILENAME: &str = "embeddings-consent.json";

/// Records that a user has acknowledged the one-off model download for
/// `model`. Stored as JSON next to the index so re-runs don't re-prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consent {
    pub model: String,
    pub accepted_at: chrono::DateTime<chrono::Utc>,
}

impl Consent {
    pub fn path(index_dir: &Path) -> PathBuf {
        index_dir.join(CONSENT_FILENAME)
    }

    pub fn load(index_dir: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(Self::path(index_dir)).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Writes (or refreshes) consent. Creates `index_dir` if needed.
    pub fn record(index_dir: &Path, model: &str) -> Result<Self, EmbedError> {
        let consent = Self {
            model: model.to_string(),
            accepted_at: chrono::Utc::now(),
        };
        let path = Self::path(index_dir);
        let json = serde_json::to_string_pretty(&consent)?;
        fs_atomic::write(&path, json.as_bytes())?;
        Ok(consent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::DEFAULT_MODEL;
    use tempfile::tempdir;

    #[test]
    fn consent_roundtrip() {
        let dir = tempdir().unwrap();
        assert!(Consent::load(dir.path()).is_none());
        let written = Consent::record(dir.path(), DEFAULT_MODEL).unwrap();
        let loaded = Consent::load(dir.path()).expect("consent should load after record");
        assert_eq!(loaded.model, DEFAULT_MODEL);
        assert_eq!(loaded.accepted_at, written.accepted_at);
    }
}
