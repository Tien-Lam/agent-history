use std::path::PathBuf;

use super::ProviderError;

/// Extracts the final path component from a path string, treating both `/`
/// and `\` as separators. Session files often record cross-platform paths
/// (e.g. `C:\Users\me\proj` written on Windows but read on Linux), so we
/// can't rely on `Path::file_name`, which only honours the host separator.
pub(crate) fn project_name_from_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let basename = trimmed.rsplit(['/', '\\']).next()?;
    if basename.is_empty() || basename.ends_with(':') {
        return None;
    }
    Some(basename.to_string())
}

/// Returns the home directory, respecting `AGHIST_HOME` env var override.
/// When `AGHIST_HOME` is set, it is used instead of the system home directory.
pub(crate) fn home_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("AGHIST_HOME") {
        return Some(PathBuf::from(home));
    }
    directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
}

pub(crate) fn discovery_error(provider: &'static str) -> impl Fn(std::io::Error) -> ProviderError {
    move |source| ProviderError::Discovery { provider, source }
}
