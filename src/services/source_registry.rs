use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::config::{self, Config, ConfigLoadError, RemoteSource, Transport};

#[derive(Debug, Error)]
pub enum SourceRegistryError {
    #[error("could not determine config path; HOME and XDG_CONFIG_HOME are unset")]
    ConfigPathUnavailable,
    #[error(transparent)]
    ConfigLoad(#[from] ConfigLoadError),
    #[error("{0}")]
    InvalidName(String),
    #[error("{0}")]
    InvalidHost(String),
    #[error("{0}")]
    InvalidPath(String),
    #[error("a source named '{0}' already exists")]
    DuplicateSource(String),
    #[error("no registered source named '{0}'")]
    SourceNotFound(String),
    #[error("failed to write {path}: {source}")]
    Save {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn resolve_config_path() -> Result<PathBuf, SourceRegistryError> {
    Config::resolved_path().ok_or(SourceRegistryError::ConfigPathUnavailable)
}

pub fn load_config(config_path: &Path) -> Result<Config, SourceRegistryError> {
    Config::try_load_from(config_path).map_err(SourceRegistryError::ConfigLoad)
}

pub fn list_remote_sources(config_path: &Path) -> Result<Vec<RemoteSource>, SourceRegistryError> {
    let config = load_config(config_path)?;
    Ok(config.sources)
}

pub fn add_remote_source(
    config_path: &Path,
    name: &str,
    host: &str,
    path: &str,
    transport: Transport,
) -> Result<RemoteSource, SourceRegistryError> {
    config::validate_source_name(name).map_err(SourceRegistryError::InvalidName)?;
    config::validate_rsync_endpoint(host, "--host").map_err(SourceRegistryError::InvalidHost)?;
    config::validate_rsync_endpoint(path, "--path").map_err(SourceRegistryError::InvalidPath)?;

    let trimmed_name = name.trim();
    let mut config = load_config(config_path)?;
    if config.sources.iter().any(|s| s.name == trimmed_name) {
        return Err(SourceRegistryError::DuplicateSource(
            trimmed_name.to_string(),
        ));
    }

    let new_source = RemoteSource {
        name: trimmed_name.to_string(),
        host: host.to_string(),
        path: path.to_string(),
        transport,
    };
    config.sources.push(new_source.clone());
    config
        .save_to(config_path)
        .map_err(|source| SourceRegistryError::Save {
            path: config_path.to_path_buf(),
            source,
        })?;
    Ok(new_source)
}

pub fn remove_remote_source(
    config_path: &Path,
    name: &str,
) -> Result<RemoteSource, SourceRegistryError> {
    let mut config = load_config(config_path)?;
    let Some(index) = config.sources.iter().position(|s| s.name == name) else {
        return Err(SourceRegistryError::SourceNotFound(name.to_string()));
    };
    let removed = config.sources.remove(index);
    config
        .save_to(config_path)
        .map_err(|source| SourceRegistryError::Save {
            path: config_path.to_path_buf(),
            source,
        })?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("config.toml")
    }

    #[test]
    fn add_persists_remote_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_path(&dir);

        let source = add_remote_source(
            &path,
            "box",
            "user@example.test",
            "/history",
            Transport::Rsync,
        )
        .unwrap();

        assert_eq!(source.name, "box");
        assert_eq!(source.transport, Transport::Rsync);
        let config = load_config(&path).unwrap();
        assert_eq!(config.sources, vec![source]);
    }

    #[test]
    fn add_rejects_duplicate_name_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_path(&dir);
        add_remote_source(&path, "box", "host-a", "/one", Transport::Ssh).unwrap();

        let err = add_remote_source(&path, "box", "host-b", "/two", Transport::Rsync).unwrap_err();

        assert!(matches!(err, SourceRegistryError::DuplicateSource(name) if name == "box"));
        let sources = list_remote_sources(&path).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].host, "host-a");
    }

    #[test]
    fn add_rejects_invalid_name_without_creating_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_path(&dir);

        let err =
            add_remote_source(&path, "../box", "host", "/history", Transport::Ssh).unwrap_err();

        assert!(matches!(err, SourceRegistryError::InvalidName(_)));
        assert!(!path.exists());
    }

    #[test]
    fn add_rejects_option_like_host_without_creating_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_path(&dir);

        let err = add_remote_source(&path, "box", "-host", "/history", Transport::Ssh).unwrap_err();

        assert!(matches!(err, SourceRegistryError::InvalidHost(_)));
        assert!(!path.exists());
    }

    #[test]
    fn remove_deletes_existing_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_path(&dir);
        add_remote_source(&path, "keep", "host", "/keep", Transport::Ssh).unwrap();
        add_remote_source(&path, "drop", "host", "/drop", Transport::Ssh).unwrap();

        let removed = remove_remote_source(&path, "drop").unwrap();

        assert_eq!(removed.name, "drop");
        let sources = list_remote_sources(&path).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "keep");
    }

    #[test]
    fn remove_unknown_source_reports_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_path(&dir);

        let err = remove_remote_source(&path, "missing").unwrap_err();

        assert!(matches!(err, SourceRegistryError::SourceNotFound(name) if name == "missing"));
    }

    #[test]
    fn load_config_surfaces_parse_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = config_path(&dir);
        std::fs::write(&path, "not = [valid").unwrap();

        let err = load_config(&path).unwrap_err();

        assert!(matches!(
            err,
            SourceRegistryError::ConfigLoad(ConfigLoadError::Parse { .. })
        ));
    }
}
