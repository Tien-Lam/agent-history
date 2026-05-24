use std::ffi::OsString;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

use super::{MetadataError, Result, ENV_PATH};

/// Resolve the metadata.db path. Honors `AGHIST_METADATA_DB`, otherwise falls
/// back to the platform data dir (`~/.local/share/aghist/metadata.db` on Linux).
pub fn default_path() -> Option<PathBuf> {
    default_path_from_env_value(std::env::var_os(ENV_PATH))
}

pub(super) fn default_path_from_env_value(override_path: Option<OsString>) -> Option<PathBuf> {
    if let Some(p) = override_path {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    directories::ProjectDirs::from("", "", "aghist").map(|dirs| dirs.data_dir().join("metadata.db"))
}

/// Schema migrations. Append new migrations; never edit or reorder existing
/// entries — `rusqlite_migration` tracks progress via `SQLite`'s `user_version`.
pub(super) fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(include_str!("0001_init.sql"))])
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
