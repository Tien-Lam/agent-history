use std::path::{Path, PathBuf};

use aghist::config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RemovalTarget {
    DefaultConfig { file: PathBuf, dir: PathBuf },
    File(PathBuf),
}

pub(super) fn config_removal_target(config_path: Option<&Path>) -> Option<RemovalTarget> {
    let default_config_path = config::Config::config_path();
    config_removal_target_for(config_path, default_config_path.as_deref())
}

fn config_removal_target_for(
    config_path: Option<&Path>,
    default_config_path: Option<&Path>,
) -> Option<RemovalTarget> {
    let path = config_path?;
    if Some(path) == default_config_path {
        let dir = path.parent()?;
        if path.exists() || dir.exists() {
            return Some(RemovalTarget::DefaultConfig {
                file: path.to_path_buf(),
                dir: dir.to_path_buf(),
            });
        }
        return None;
    }
    if path.exists() {
        Some(RemovalTarget::File(path.to_path_buf()))
    } else {
        None
    }
}

impl RemovalTarget {
    pub(super) fn display_path(&self) -> &Path {
        match self {
            Self::DefaultConfig { file, .. } | Self::File(file) => file,
        }
    }

    pub(super) fn remove(&self) -> std::io::Result<()> {
        match self {
            Self::DefaultConfig { file, dir } => {
                remove_file_if_exists(file)?;
                remove_dir_if_empty(dir)
            }
            Self::File(path) => std::fs::remove_file(path),
        }
    }
}

fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn remove_dir_if_empty(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_target_removes_config_file_and_empty_dir() {
        let root = tempfile::tempdir().unwrap();
        let config_dir = root.path().join("aghist");
        let config_path = config_dir.join("config.toml");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(&config_path, "cache_size = 20\n").unwrap();

        let target = config_removal_target_for(Some(&config_path), Some(&config_path))
            .expect("default config dir should be targeted");

        assert_eq!(
            target,
            RemovalTarget::DefaultConfig {
                file: config_path.clone(),
                dir: config_dir.clone()
            }
        );
        target.remove().unwrap();
        assert!(!config_path.exists());
        assert!(!config_dir.exists());
    }

    #[test]
    fn default_config_target_preserves_unknown_files() {
        let root = tempfile::tempdir().unwrap();
        let config_dir = root.path().join("aghist");
        let config_path = config_dir.join("config.toml");
        let keep_path = config_dir.join("keep.txt");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(&config_path, "cache_size = 20\n").unwrap();
        std::fs::write(&keep_path, "keep\n").unwrap();

        let target = config_removal_target_for(Some(&config_path), Some(&config_path))
            .expect("default config dir should be targeted");

        target.remove().unwrap();
        assert!(!config_path.exists());
        assert!(config_dir.exists());
        assert!(keep_path.exists());
    }

    #[test]
    fn override_config_target_removes_only_config_file() {
        let root = tempfile::tempdir().unwrap();
        let shared_dir = root.path().join("shared");
        let config_path = shared_dir.join("aghist.toml");
        let keep_path = shared_dir.join("keep.txt");
        let default_config_path = root.path().join("default").join("config.toml");
        std::fs::create_dir_all(&shared_dir).unwrap();
        std::fs::write(&config_path, "cache_size = 20\n").unwrap();
        std::fs::write(&keep_path, "keep\n").unwrap();

        let target = config_removal_target_for(Some(&config_path), Some(&default_config_path))
            .expect("existing override config file should be targeted");

        assert_eq!(target, RemovalTarget::File(config_path.clone()));
        target.remove().unwrap();
        assert!(!config_path.exists());
        assert!(shared_dir.exists());
        assert!(keep_path.exists());
    }

    #[test]
    fn missing_override_config_has_no_removal_target() {
        let root = tempfile::tempdir().unwrap();
        let config_path = root.path().join("shared").join("aghist.toml");
        let default_config_path = root.path().join("default").join("config.toml");

        let target = config_removal_target_for(Some(&config_path), Some(&default_config_path));

        assert_eq!(target, None);
    }
}
