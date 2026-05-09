//! User-annotation sidecar database.
//!
//! aghist must never modify a provider's session files (Claude Code's JSONL,
//! Copilot's logs, etc). Per-user annotations — notes, tags, stars — live in a
//! separate sqlite database keyed by stable citation refs of the form
//! `<provider>/<session-id>` or `<provider>/<session-id>#<turn>`.
//!
//! Default location is `~/.local/share/aghist/metadata.db` (`XDG_DATA_HOME` on
//! Linux, `Library/Application Support` on macOS, `%APPDATA%` on Windows).
//! Override with `AGHIST_METADATA_DB`.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use thiserror::Error;

const ENV_PATH: &str = "AGHIST_METADATA_DB";

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("could not resolve metadata.db path; set {ENV_PATH} or ensure XDG/home dirs exist")]
    NoPath,
    #[error("create parent directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("open {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("apply migrations on {path}: {source}")]
    Migrate {
        path: PathBuf,
        #[source]
        source: rusqlite_migration::Error,
    },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, MetadataError>;

/// Resolve the metadata.db path. Honors `AGHIST_METADATA_DB`, otherwise falls
/// back to the platform data dir (`~/.local/share/aghist/metadata.db` on Linux).
pub fn default_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(ENV_PATH) {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    directories::ProjectDirs::from("", "", "aghist")
        .map(|dirs| dirs.data_dir().join("metadata.db"))
}

/// Schema migrations. Append new migrations; never edit or reorder existing
/// entries — `rusqlite_migration` tracks progress via `SQLite`'s `user_version`.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(include_str!("metadata/0001_init.sql"))])
}

/// Open the metadata database at `path`, creating parent directories and
/// applying any pending migrations. Idempotent: safe to call on every startup.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|source| MetadataError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    let mut conn = Connection::open(path).map_err(|source| MetadataError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrations()
        .to_latest(&mut conn)
        .map_err(|source| MetadataError::Migrate {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(conn)
}

/// Open the metadata database at the resolved default path.
pub fn open_default() -> Result<Connection> {
    let path = default_path().ok_or(MetadataError::NoPath)?;
    open(&path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn migrations_validate() {
        migrations().validate().expect("migrations are valid");
    }

    #[test]
    fn open_creates_db_and_parent_dir() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested/dir/metadata.db");
        let conn = open(&path).unwrap();
        assert!(path.exists());
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|n| !n.starts_with("sqlite_"))
            .collect();
        assert!(tables.contains(&"notes".to_string()), "tables = {tables:?}");
        assert!(tables.contains(&"tags".to_string()), "tables = {tables:?}");
        assert!(tables.contains(&"stars".to_string()), "tables = {tables:?}");
    }

    #[test]
    fn open_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("metadata.db");
        let _ = open(&path).unwrap();
        let conn = open(&path).unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert!(version >= 1, "user_version should be set after migration");
    }

    #[test]
    fn schema_supports_basic_inserts() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("metadata.db");
        let conn = open(&path).unwrap();
        conn.execute(
            "INSERT INTO notes(session_ref, body) VALUES (?1, ?2)",
            ("claude-code/abc123#42", "first note"),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
            ("claude-code/abc123", "review"),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO stars(session_ref) VALUES (?1)",
            ["claude-code/abc123"],
        )
        .unwrap();

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn stars_are_unique_per_session_ref() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("metadata.db");
        let conn = open(&path).unwrap();
        conn.execute(
            "INSERT INTO stars(session_ref) VALUES (?1)",
            ["claude-code/abc"],
        )
        .unwrap();
        let dup = conn.execute(
            "INSERT INTO stars(session_ref) VALUES (?1)",
            ["claude-code/abc"],
        );
        assert!(dup.is_err(), "duplicate star should violate UNIQUE");
    }

    #[test]
    fn tags_are_unique_per_session_ref_tag_pair() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("metadata.db");
        let conn = open(&path).unwrap();
        conn.execute(
            "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
            ("claude-code/abc", "review"),
        )
        .unwrap();
        let dup = conn.execute(
            "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
            ("claude-code/abc", "review"),
        );
        assert!(dup.is_err(), "duplicate (session_ref,tag) should violate UNIQUE");

        // Same session_ref with a different tag is allowed.
        conn.execute(
            "INSERT INTO tags(session_ref, tag) VALUES (?1, ?2)",
            ("claude-code/abc", "todo"),
        )
        .unwrap();
    }

    #[test]
    fn env_var_overrides_default_path() {
        let tmp = TempDir::new().unwrap();
        let custom = tmp.path().join("custom.db");
        // SAFETY: tests run sequentially within a test binary by default; the
        // env var is set and read here only.
        std::env::set_var(ENV_PATH, &custom);
        let resolved = default_path().expect("path resolves with env var set");
        std::env::remove_var(ENV_PATH);
        assert_eq!(resolved, custom);
    }
}
