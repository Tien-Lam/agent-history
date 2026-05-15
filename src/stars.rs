//! Persistent per-session bookmarks ("stars").
//!
//! Stars are stored in the same `SQLite` metadata sidecar as notes and tags.
//! The TUI keeps a small in-memory cache for rendering speed and writes
//! through to `SQLite` on every toggle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::metadata;
use crate::model::{Provider, SessionId, SessionRef};

/// In-memory star set, optionally backed by `metadata.db`.
#[derive(Debug, Default, Clone)]
pub struct StarStore {
    path: Option<PathBuf>,
    starred: HashMap<(Provider, String), DateTime<Utc>>,
}

impl StarStore {
    /// Load from the default metadata sidecar (`AGHIST_METADATA_DB` or the
    /// platform data dir). If no path can be resolved, the store is in-memory.
    pub fn load_default() -> Self {
        let path = metadata::default_path();
        let mut store = Self {
            path: path.clone(),
            starred: HashMap::new(),
        };
        if let Some(path) = path.as_ref() {
            store.reload_from(path);
        }
        store
    }

    /// Load from an explicit metadata DB path. Tests use this to avoid the
    /// user's real metadata sidecar.
    pub fn load_from(path: &Path) -> Self {
        let mut store = Self {
            path: Some(path.to_path_buf()),
            starred: HashMap::new(),
        };
        store.reload_from(path);
        store
    }

    /// In-memory only; toggles never touch disk.
    pub fn ephemeral() -> Self {
        Self {
            path: None,
            starred: HashMap::new(),
        }
    }

    fn reload_from(&mut self, path: &Path) {
        let Ok(conn) = metadata::open(path) else {
            return;
        };
        let Ok(stars) = metadata::star_list(&conn, None) else {
            return;
        };
        self.starred = stars
            .into_iter()
            .filter_map(|star| parse_session_ref(&star.session_ref).map(|key| (key, star)))
            .filter_map(|((provider, session_id), star)| {
                parse_starred_at(&star.starred_at).map(|ts| ((provider, session_id), ts))
            })
            .collect();
    }

    pub fn is_starred(&self, provider: Provider, session_id: &str) -> bool {
        self.starred
            .contains_key(&(provider, session_id.to_string()))
    }

    pub fn count(&self) -> usize {
        self.starred.len()
    }

    /// Toggle the star for `(provider, session_id)` and persist. Returns the
    /// new state (`true` = starred).
    pub fn toggle(&mut self, provider: Provider, session_id: &str) -> std::io::Result<bool> {
        let key = (provider, session_id.to_string());
        let session_ref = session_ref(provider, session_id);

        if self.starred.remove(&key).is_some() {
            self.remove_persisted(&session_ref)?;
            return Ok(false);
        }

        let starred_at = self.add_persisted(&session_ref)?;
        self.starred.insert(key, starred_at);
        Ok(true)
    }

    fn add_persisted(&self, session_ref: &str) -> std::io::Result<DateTime<Utc>> {
        let Some(path) = &self.path else {
            return Ok(Utc::now());
        };
        let conn = metadata::open(path).map_err(metadata_io_error)?;
        match metadata::star_add(&conn, session_ref) {
            Ok(star) => Ok(parse_starred_at(&star.starred_at).unwrap_or_else(Utc::now)),
            Err(metadata::MetadataError::StarAlreadyExists { .. }) => Ok(Utc::now()),
            Err(e) => Err(metadata_io_error(e)),
        }
    }

    fn remove_persisted(&self, session_ref: &str) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let conn = metadata::open(path).map_err(metadata_io_error)?;
        match metadata::star_remove(&conn, session_ref) {
            Ok(_) | Err(metadata::MetadataError::StarNotFound { .. }) => Ok(()),
            Err(e) => Err(metadata_io_error(e)),
        }
    }
}

fn session_ref(provider: Provider, session_id: &str) -> String {
    SessionRef::new(provider, SessionId(session_id.to_string())).map_or_else(
        || format!("{}/{}", provider.slug(), session_id),
        |r| r.to_string(),
    )
}

fn parse_session_ref(raw: &str) -> Option<(Provider, String)> {
    raw.parse::<SessionRef>()
        .ok()
        .map(|r| (r.provider, r.session_id.0))
}

fn parse_starred_at(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn metadata_io_error(error: metadata::MetadataError) -> std::io::Error {
    std::io::Error::other(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn toggle_round_trips_through_metadata_db() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("metadata.db");

        let mut store = StarStore::load_from(&path);
        assert_eq!(store.count(), 0);
        assert!(!store.is_starred(Provider::ClaudeCode, "abc"));

        let now = store.toggle(Provider::ClaudeCode, "abc").unwrap();
        assert!(now);
        assert!(store.is_starred(Provider::ClaudeCode, "abc"));
        assert_eq!(store.count(), 1);

        let store2 = StarStore::load_from(&path);
        assert_eq!(store2.count(), 1);
        assert!(store2.is_starred(Provider::ClaudeCode, "abc"));
    }

    #[test]
    fn toggle_off_removes_entry() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("metadata.db");

        let mut store = StarStore::load_from(&path);
        store.toggle(Provider::CodexCli, "xyz").unwrap();
        let now = store.toggle(Provider::CodexCli, "xyz").unwrap();
        assert!(!now);
        assert_eq!(store.count(), 0);

        let store2 = StarStore::load_from(&path);
        assert_eq!(store2.count(), 0);
    }

    #[test]
    fn ephemeral_does_not_write() {
        let mut store = StarStore::ephemeral();
        store.toggle(Provider::OpenCode, "id").unwrap();
        assert!(store.is_starred(Provider::OpenCode, "id"));
    }

    #[test]
    fn turn_level_stars_are_ignored_by_tui_cache() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("metadata.db");
        let conn = metadata::open(&path).unwrap();
        metadata::star_add(&conn, "claude-code/abc#7").unwrap();

        let store = StarStore::load_from(&path);
        assert_eq!(store.count(), 0);
        assert!(!store.is_starred(Provider::ClaudeCode, "abc"));
    }
}
