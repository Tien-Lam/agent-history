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

pub(crate) fn env_path(var: &str) -> Option<PathBuf> {
    path_from_env_value(std::env::var_os(var))
}

pub(crate) fn env_var_is_non_empty(var: &str) -> bool {
    env_value_is_non_empty(std::env::var_os(var))
}

pub(crate) fn discovery_error(provider: &'static str) -> impl Fn(std::io::Error) -> ProviderError {
    move |source| ProviderError::Discovery { provider, source }
}

fn home_dir_from_env_value(
    override_home: Option<OsString>,
    fallback: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    path_from_env_value(override_home).or_else(fallback)
}

fn path_from_env_value(override_path: Option<OsString>) -> Option<PathBuf> {
    override_path
        .filter(|path| !os_string_is_blank(path))
        .map(PathBuf::from)
}

fn env_value_is_non_empty(value: Option<OsString>) -> bool {
    value.is_some_and(|value| !os_string_is_blank(&value))
}

fn os_string_is_blank(value: &OsString) -> bool {
    value.is_empty() || value.to_string_lossy().trim().is_empty()
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

    #[test]
    fn path_from_env_value_ignores_empty_override() {
        assert_eq!(path_from_env_value(Some(OsString::new())), None);
    }

    #[test]
    fn path_from_env_value_ignores_blank_override() {
        assert_eq!(path_from_env_value(Some(OsString::from(" \t "))), None);
    }

    #[test]
    fn path_from_env_value_uses_non_empty_override() {
        assert_eq!(
            path_from_env_value(Some(OsString::from("/override/path"))),
            Some(PathBuf::from("/override/path"))
        );
    }

    #[test]
    fn env_value_is_non_empty_treats_empty_as_unset() {
        assert!(!env_value_is_non_empty(None));
        assert!(!env_value_is_non_empty(Some(OsString::new())));
        assert!(!env_value_is_non_empty(Some(OsString::from(" \t "))));
        assert!(env_value_is_non_empty(Some(OsString::from("/override"))));
    }
}
