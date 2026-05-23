use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("index error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("query parse error: {0}")]
    QueryParse(#[from] tantivy::query::QueryParserError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "refusing to reset index directory {path}: it contains non-aghist entries ({entries})"
    )]
    UnsafeIndexDir { path: PathBuf, entries: String },
}
