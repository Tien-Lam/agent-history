use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("failed to discover sessions for {provider}: {source}")]
    Discovery {
        provider: &'static str,
        source: std::io::Error,
    },

    #[error("failed to parse session at {}: {reason}", path.display())]
    Parse { path: PathBuf, reason: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
