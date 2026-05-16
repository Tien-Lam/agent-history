//! User-annotation sidecar database.
//!
//! aghist must never modify a provider's session files (Claude Code's JSONL,
//! Copilot's logs, etc). Per-user annotations — notes, tags, stars — live in a
//! separate sqlite database keyed by stable citation refs of the form
//! `<provider>/<session-id>`, `<provider>/<session-id>#<turn>`, or the same
//! refs prefixed with `<source>:` for remote-source sessions.
//!
//! Default location is `~/.local/share/aghist/metadata.db` (`XDG_DATA_HOME` on
//! Linux, `Library/Application Support` on macOS, `%APPDATA%` on Windows).
//! Override with `AGHIST_METADATA_DB`.

mod connection;
mod filters;
mod notes;
mod refs;
mod stars;
mod tags;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

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
    #[error("invalid session ref '{0}': {1}")]
    InvalidSessionRef(String, &'static str),
    #[error("note body must not be empty")]
    EmptyBody,
    #[error("note id {0} not found")]
    NoteNotFound(i64),
    #[error("tag must not be empty")]
    EmptyTag,
    #[error("tag '{tag}' is already attached to {session_ref}")]
    TagAlreadyExists { session_ref: String, tag: String },
    #[error("tag '{tag}' is not attached to {session_ref}")]
    TagNotFound { session_ref: String, tag: String },
    #[error("{session_ref} is already starred")]
    StarAlreadyExists { session_ref: String },
    #[error("{session_ref} is not starred")]
    StarNotFound { session_ref: String },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, MetadataError>;

pub use connection::{default_path, open, open_default};
pub use filters::filter_session_keys;
pub use notes::{note_add, note_edit, note_get, note_list, note_remove, Note};
pub use refs::validate_session_ref;
pub use stars::{star_add, star_get, star_list, star_remove, Star};
pub use tags::{tag_add, tag_list, tag_remove, Tag};
