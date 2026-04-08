use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("ambiguous session prefix '{prefix}' matches {count} sessions")]
    AmbiguousPrefix { prefix: String, count: usize },

    #[error("failed to read session at {path}")]
    ReadSession {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to parse entry in {path} at line {line}")]
    ParseEntry {
        path: PathBuf,
        line: usize,
        #[source]
        source: serde_json::Error,
    },

    #[error(transparent)]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
