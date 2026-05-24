use std::io;
use std::path::{Path, PathBuf};

use aghist::cli_error::ErrorEnvelope;
use aghist::config;

use super::super::super::dir_accounting;

pub(super) fn resolve_sources_cache_root() -> Result<PathBuf, ErrorEnvelope> {
    config::sources_cache_root().ok_or_else(|| {
        ErrorEnvelope::new(
            "config-error",
            "could not determine sources cache dir; HOME and XDG_CACHE_HOME are unset",
        )
        .with_hint("Set AGHIST_SOURCES_CACHE_DIR=/path/to/cache to override.")
    })
}

pub(super) fn ensure_cache_root_safe(cache_root: &Path) -> Result<(), ErrorEnvelope> {
    ensure_cache_dir(cache_root, "sources cache root")
}

pub(super) fn ensure_cache_dir(path: &Path, label: &str) -> Result<(), ErrorEnvelope> {
    ensure_existing_cache_dir_safe(path, label)?;
    std::fs::create_dir_all(path).map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to create {label} {}: {e}", path.display()),
        )
    })?;
    ensure_existing_cache_dir_safe(path, label)
}

fn ensure_existing_cache_dir_safe(path: &Path, label: &str) -> Result<(), ErrorEnvelope> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(ErrorEnvelope::new(
            "unsafe-cache-dir",
            format!("{label} {} is a symlink", path.display()),
        )
        .with_hint("Remove the symlink and retry; aghist will create an owned cache directory.")),
        Ok(meta) if !meta.is_dir() => Err(ErrorEnvelope::new(
            "unsafe-cache-dir",
            format!("{label} {} is not a directory", path.display()),
        )
        .with_hint("Remove the path and retry; aghist will create an owned cache directory.")),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ErrorEnvelope::new(
            "io-error",
            format!("failed to inspect {label} {}: {e}", path.display()),
        )),
    }
}

/// Iterative `(file_count, total_bytes)`. Symlinks are skipped.
pub(super) fn count_dir(dir: &Path) -> Result<(u64, u64), ErrorEnvelope> {
    dir_accounting::dir_stats(dir)
        .map(|stats| (stats.files, stats.bytes))
        .map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to account source data dir {}: {e}", dir.display()),
            )
        })
}

#[cfg(all(test, unix))]
mod tests {
    use super::count_dir;
    use std::os::unix::fs::symlink;

    #[test]
    fn dir_accounting_skips_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("real.txt"), "12345").unwrap();
        symlink(root, root.join("loop")).unwrap();
        symlink(root.join("real.txt"), root.join("file-link")).unwrap();

        assert_eq!(count_dir(root).unwrap(), (1, 5));
    }

    #[test]
    fn dir_accounting_reports_missing_root() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");

        let err = count_dir(&missing).unwrap_err();

        assert_eq!(err.kind, "io-error");
        assert!(
            err.message.contains("failed to account source data dir"),
            "unexpected error: {err:?}"
        );
    }
}
