use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DirStats {
    pub(super) files: u64,
    pub(super) bytes: u64,
}

const MAX_ACCOUNTING_FILES: usize = 1_000_000;
const MAX_ACCOUNTING_DIRS: usize = 1_000_000;

/// Iterative directory accounting. Symlinked children are skipped.
pub(super) fn dir_stats(root: &Path) -> io::Result<DirStats> {
    dir_stats_limited(root, MAX_ACCOUNTING_FILES, MAX_ACCOUNTING_DIRS)
}

fn dir_stats_limited(root: &Path, max_files: usize, max_dirs: usize) -> io::Result<DirStats> {
    let mut stats = DirStats { files: 0, bytes: 0 };
    let mut pending = vec![root.to_path_buf()];
    let mut visited_dirs = 0usize;

    while let Some(dir) = pending.pop() {
        visited_dirs = visited_dirs.saturating_add(1);
        if visited_dirs > max_dirs {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} has too many directories to account (>{max_dirs})",
                    root.display()
                ),
            ));
        }
        let entries = std::fs::read_dir(&dir)
            .map_err(|error| io_with_path("read directory", &dir, &error))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| io_with_path("read directory entry in", &dir, &error))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| io_with_path("read file type for", &path, &error))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_file() {
                let next_files = stats.files.saturating_add(1);
                if next_files > max_files as u64 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "{} has too many files to account (>{max_files})",
                            root.display()
                        ),
                    ));
                }
                let meta = entry
                    .metadata()
                    .map_err(|error| io_with_path("read metadata for", &path, &error))?;
                stats.files = next_files;
                stats.bytes = stats.bytes.saturating_add(meta.len());
            } else if file_type.is_dir() {
                pending.push(path);
            }
        }
    }

    Ok(stats)
}

pub(super) fn dir_size_bytes(dir: &Path) -> io::Result<u64> {
    dir_stats(dir).map(|stats| stats.bytes)
}

fn io_with_path(action: &str, path: &Path, error: &io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("{action} {}: {error}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::{dir_size_bytes, dir_stats_limited};

    #[cfg(unix)]
    use super::{dir_stats, DirStats};
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[test]
    fn reports_non_directory_roots() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not-a-dir");
        std::fs::write(&file, "content").unwrap();

        let err = dir_size_bytes(&file).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::NotADirectory);
        assert!(err.to_string().contains("not-a-dir"));
    }

    #[cfg(unix)]
    #[test]
    fn skips_symlinked_children() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("real.txt"), "12345").unwrap();
        symlink(root, root.join("loop")).unwrap();
        symlink(root.join("real.txt"), root.join("file-link")).unwrap();

        assert_eq!(dir_stats(root).unwrap(), DirStats { files: 1, bytes: 5 });
        assert_eq!(dir_size_bytes(root).unwrap(), 5);
    }

    #[test]
    fn rejects_too_many_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::write(dir.path().join("b.txt"), "b").unwrap();

        let err = dir_stats_limited(dir.path(), 1, 10).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("too many files"));
    }

    #[test]
    fn rejects_too_many_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("child")).unwrap();

        let err = dir_stats_limited(dir.path(), 10, 1).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("too many directories"));
    }
}
