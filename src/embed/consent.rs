use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::EmbedError;
use crate::fs_atomic;
use crate::fs_read;

const CONSENT_FILENAME: &str = "embeddings-consent.json";
const MAX_CONSENT_BYTES: usize = 64 * 1024;

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

    pub fn read(index_dir: &Path) -> Result<Option<Self>, EmbedError> {
        let path = Self::path(index_dir);
        let raw = match fs_read::read_to_string_limited(&path, MAX_CONSENT_BYTES) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        serde_json::from_str(&raw).map(Some).map_err(Into::into)
    }

    pub fn load(index_dir: &Path) -> Option<Self> {
        Self::read(index_dir).ok().flatten()
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

    #[test]
    fn read_reports_corrupt_existing_consent() {
        let dir = tempdir().unwrap();
        std::fs::write(Consent::path(dir.path()), b"not json").unwrap();

        assert!(Consent::load(dir.path()).is_none());
        assert!(matches!(
            Consent::read(dir.path()),
            Err(EmbedError::Json(_))
        ));
    }
}
