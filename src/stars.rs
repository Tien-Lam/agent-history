//! Persistent per-session bookmarks ("stars").
//!
//! Stars are keyed by `(provider, session_id)` and persisted to a TOML file at
//! `~/.config/aghist/stars.toml` (overridable via `AGHIST_STARS_PATH`). Writes
//! are atomic — TOML is rendered to a sibling temp file then renamed into place.
//!
//! When no path is configured (e.g. the system has no XDG/HOME and no env var),
//! the store runs in-memory only: toggles work but nothing is persisted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::Provider;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StarsFile {
    #[serde(default, rename = "stars")]
    entries: Vec<StarEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StarEntry {
    provider: String,
    session_id: String,
    #[serde(default = "Utc::now")]
    starred_at: DateTime<Utc>,
}

/// In-memory star set, optionally backed by a TOML file.
#[derive(Debug, Default, Clone)]
pub struct StarStore {
    path: Option<PathBuf>,
    starred: HashMap<(Provider, String), DateTime<Utc>>,
}

impl StarStore {
    /// Load from the default path (`AGHIST_STARS_PATH` or
    /// `~/.config/aghist/stars.toml`). Missing or unreadable files yield an
    /// empty store; the path is still recorded so subsequent toggles persist.
    pub fn load_default() -> Self {
        let path = default_stars_path();
        let mut store = Self {
            path: path.clone(),
            starred: HashMap::new(),
        };
        if let Some(p) = path.as_ref() {
            store.reload_from(p);
        }
        store
    }

    /// Load from an explicit path (for tests).
    pub fn load_from(path: &Path) -> Self {
        let mut store = Self {
            path: Some(path.to_path_buf()),
            starred: HashMap::new(),
        };
        store.reload_from(path);
        store
    }

    /// In-memory only; toggles never touch disk. Useful for transient/test
    /// scenarios.
    pub fn ephemeral() -> Self {
        Self {
            path: None,
            starred: HashMap::new(),
        }
    }

    fn reload_from(&mut self, path: &Path) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let Ok(file) = toml::from_str::<StarsFile>(&text) else {
            return;
        };
        self.starred = file
            .entries
            .into_iter()
            .filter_map(|e| Provider::from_slug(&e.provider).map(|p| ((p, e.session_id), e.starred_at)))
            .collect();
    }

    pub fn is_starred(&self, provider: Provider, session_id: &str) -> bool {
        // Key by reference would require a different map shape; allocate a
        // String for the lookup. Toggle/star operations are user-paced
        // (keystrokes), so the cost is negligible.
        self.starred
            .contains_key(&(provider, session_id.to_string()))
    }

    pub fn count(&self) -> usize {
        self.starred.len()
    }

    /// Toggle the star for `(provider, session_id)` and persist. Returns the
    /// new state (`true` = starred). Persistence errors propagate up so
    /// callers can surface them; the in-memory state still reflects the toggle.
    pub fn toggle(&mut self, provider: Provider, session_id: &str) -> std::io::Result<bool> {
        let key = (provider, session_id.to_string());
        let now_starred = if self.starred.remove(&key).is_some() {
            false
        } else {
            self.starred.insert(key, Utc::now());
            true
        };
        self.persist()?;
        Ok(now_starred)
    }

    fn persist(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let mut entries: Vec<StarEntry> = self
            .starred
            .iter()
            .map(|((p, id), ts)| StarEntry {
                provider: p.slug().to_string(),
                session_id: id.clone(),
                starred_at: *ts,
            })
            .collect();
        // Stable order — keeps diffs readable and round-trips deterministic.
        entries.sort_by(|a, b| {
            (a.provider.as_str(), a.session_id.as_str())
                .cmp(&(b.provider.as_str(), b.session_id.as_str()))
        });
        let file = StarsFile { entries };
        let toml = toml::to_string_pretty(&file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, toml)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// Default storage location: `AGHIST_STARS_PATH` if set, else
/// `<config dir>/aghist/stars.toml`. Returns `None` when neither is available.
pub fn default_stars_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AGHIST_STARS_PATH") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    directories::ProjectDirs::from("", "", "aghist")
        .map(|dirs| dirs.config_dir().join("stars.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn toggle_round_trips_through_disk() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("stars.toml");

        let mut store = StarStore::load_from(&path);
        assert_eq!(store.count(), 0);
        assert!(!store.is_starred(Provider::ClaudeCode, "abc"));

        let now = store.toggle(Provider::ClaudeCode, "abc").unwrap();
        assert!(now);
        assert!(store.is_starred(Provider::ClaudeCode, "abc"));
        assert_eq!(store.count(), 1);

        // Reload from disk — the star should survive.
        let store2 = StarStore::load_from(&path);
        assert_eq!(store2.count(), 1);
        assert!(store2.is_starred(Provider::ClaudeCode, "abc"));
    }

    #[test]
    fn toggle_off_removes_entry() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("stars.toml");

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
    fn malformed_file_yields_empty_store() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("stars.toml");
        std::fs::write(&path, "this is not = valid [[ toml").unwrap();
        let store = StarStore::load_from(&path);
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn unknown_provider_slugs_are_dropped() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("stars.toml");
        std::fs::write(
            &path,
            r#"
[[stars]]
provider = "claude-code"
session_id = "keep-me"
starred_at = "2026-01-01T00:00:00Z"

[[stars]]
provider = "not-a-provider"
session_id = "drop-me"
starred_at = "2026-01-01T00:00:00Z"
"#,
        )
        .unwrap();
        let store = StarStore::load_from(&path);
        assert_eq!(store.count(), 1);
        assert!(store.is_starred(Provider::ClaudeCode, "keep-me"));
    }

    #[test]
    fn persisted_entries_are_sorted() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("stars.toml");
        let mut store = StarStore::load_from(&path);
        store.toggle(Provider::OpenCode, "z").unwrap();
        store.toggle(Provider::ClaudeCode, "b").unwrap();
        store.toggle(Provider::ClaudeCode, "a").unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let pos_a = text.find("\"a\"").expect("session a");
        let pos_b = text.find("\"b\"").expect("session b");
        let pos_z = text.find("\"z\"").expect("session z");
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_z);
    }
}
