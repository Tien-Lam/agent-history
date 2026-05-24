use std::ffi::OsString;
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
    home_dir_from_env_value(std::env::var_os("AGHIST_HOME"), || {
        directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
    })
}

pub(crate) fn discovery_error(provider: &'static str) -> impl Fn(std::io::Error) -> ProviderError {
    move |source| ProviderError::Discovery { provider, source }
}

fn home_dir_from_env_value(
    override_home: Option<OsString>,
    fallback: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(home) = override_home {
        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    fallback()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_dir_ignores_empty_env_override() {
        assert_eq!(
            home_dir_from_env_value(Some(OsString::new()), || Some(PathBuf::from(
                "/fallback/home"
            ))),
            Some(PathBuf::from("/fallback/home"))
        );
    }

    #[test]
    fn home_dir_uses_non_empty_env_override() {
        assert_eq!(
            home_dir_from_env_value(Some(OsString::from("/override/home")), || Some(
                PathBuf::from("/fallback/home")
            )),
            Some(PathBuf::from("/override/home"))
        );
    }
}
