use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::config;
use aghist::output::OutputMode;
use chrono::{DateTime, Utc};

use super::super::format_bytes;
use super::resolve_config_path;

fn resolve_sources_cache_root() -> Result<PathBuf, ErrorEnvelope> {
    config::sources_cache_root().ok_or_else(|| {
        ErrorEnvelope::new(
            "config-error",
            "could not determine sources cache dir; HOME and XDG_CACHE_HOME are unset",
        )
        .with_hint("Set AGHIST_SOURCES_CACHE_DIR=/path/to/cache to override.")
    })
}

#[derive(serde::Serialize)]
struct PullResult {
    name: String,
    host: String,
    path: String,
    transport: String,
    data_dir: String,
    dry_run: bool,
    byte_count: u64,
    file_count: u64,
    pulled_at: DateTime<Utc>,
}

pub(crate) fn sources_pull_remote(
    name: Option<&str>,
    all: bool,
    dry_run: bool,
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    let config_path = resolve_config_path()?;
    let config = config::Config::load_from(&config_path);

    let targets: Vec<config::RemoteSource> = match (name, all) {
        (Some(n), false) => {
            let trimmed = n.trim();
            let Some(found) = config.sources.iter().find(|s| s.name == trimmed) else {
                return Err(ErrorEnvelope::new(
                    "source-not-found",
                    format!("no registered source named '{trimmed}'"),
                )
                .with_hint("Run `aghist sources list` to see registered sources."));
            };
            vec![found.clone()]
        }
        (None, true) => {
            if config.sources.is_empty() {
                return Err(ErrorEnvelope::new(
                    "source-not-found",
                    "no remote sources are registered",
                )
                .with_hint(
                    "Add one with `aghist sources add <name> --host <host> --path <path>`.",
                ));
            }
            config.sources.clone()
        }
        (None, false) => {
            return Err(
                ErrorEnvelope::new("usage", "must pass either <NAME> or --all")
                    .with_hint("Run `aghist sources pull --help` for usage."),
            );
        }
        (Some(_), true) => {
            return Err(ErrorEnvelope::new(
                "usage",
                "<NAME> and --all are mutually exclusive",
            ));
        }
    };

    let cache_root = resolve_sources_cache_root()?;
    let mut results = Vec::with_capacity(targets.len());
    for src in targets {
        let result = pull_one_source(&src, &cache_root, dry_run)?;
        results.push(result);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_pull_results(&mut out, &results, &cache_root, mode)
        .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write pull output: {e}")))?;
    out.flush()
        .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to flush pull output: {e}")))?;
    Ok(EXIT_OK)
}

fn pull_one_source(
    src: &config::RemoteSource,
    cache_root: &Path,
    dry_run: bool,
) -> Result<PullResult, ErrorEnvelope> {
    src.validate()
        .map_err(|message| ErrorEnvelope::new("usage", message))?;
    let source_dir = src.cache_dir(cache_root);
    ensure_existing_cache_dir_safe(&source_dir, "source cache dir")?;
    std::fs::create_dir_all(&source_dir).map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to create cache dir {}: {e}", source_dir.display()),
        )
    })?;

    let data_dir = src.data_dir(cache_root);
    ensure_existing_cache_dir_safe(&data_dir, "source data dir")?;
    std::fs::create_dir_all(&data_dir).map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to create cache dir {}: {e}", data_dir.display()),
        )
    })?;

    let rsync_bin = std::env::var("AGHIST_RSYNC_BIN").unwrap_or_else(|_| "rsync".to_string());
    let remote = build_rsync_remote_url(src);
    let mut local = data_dir.display().to_string();
    if !local.ends_with('/') {
        local.push('/');
    }

    let mut cmd = std::process::Command::new(&rsync_bin);
    cmd.arg("-a").arg("--delete");
    if dry_run {
        cmd.arg("--dry-run");
    }
    if matches!(src.transport, config::Transport::Ssh) {
        cmd.arg("-e").arg("ssh -o BatchMode=yes");
    }
    cmd.arg("--").arg(&remote).arg(&local);

    let output = cmd.output().map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to invoke rsync ('{rsync_bin}'): {e}"),
        )
        .with_hint("Install rsync, or set AGHIST_RSYNC_BIN to a working binary.")
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output
            .status
            .code()
            .map_or_else(|| String::from("?"), |c| c.to_string());
        return Err(ErrorEnvelope::new(
            "rsync-failed",
            format!("rsync exited {code} for source '{}'", src.name),
        )
        .with_hint(format!(
            "remote: {remote} - stderr: {}",
            stderr.lines().last().unwrap_or("").trim()
        )));
    }

    let (file_count, byte_count) = if dry_run {
        (0, 0)
    } else {
        count_dir(&data_dir)
    };
    let pulled_at = Utc::now();
    let manifest = config::SourceCacheManifest {
        name: src.name.clone(),
        host: src.host.clone(),
        path: src.path.clone(),
        transport: src.transport,
        data_dir: data_dir.display().to_string(),
        last_pulled_at: pulled_at,
        last_pull_dry_run: dry_run,
        byte_count,
        file_count,
    };
    let manifest_path = src.manifest_path(cache_root);
    manifest.save(&manifest_path).map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to write manifest {}: {e}", manifest_path.display()),
        )
    })?;

    Ok(PullResult {
        name: src.name.clone(),
        host: src.host.clone(),
        path: src.path.clone(),
        transport: src.transport.slug().to_string(),
        data_dir: data_dir.display().to_string(),
        dry_run,
        byte_count,
        file_count,
        pulled_at,
    })
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

fn build_rsync_remote_url(src: &config::RemoteSource) -> String {
    let path = src.path.trim_end_matches('/');
    match src.transport {
        config::Transport::Ssh => format!("{}:{}/", src.host, path),
        config::Transport::Rsync => {
            let path = path.trim_start_matches('/');
            format!("rsync://{}/{}/", src.host, path)
        }
    }
}

/// Recursive `(file_count, total_bytes)`. Symlinks and IO errors are skipped.
fn count_dir(dir: &Path) -> (u64, u64) {
    let mut files: u64 = 0;
    let mut bytes: u64 = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_file() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            files = files.saturating_add(1);
            bytes = bytes.saturating_add(meta.len());
        } else if file_type.is_dir() {
            let (f, b) = count_dir(&entry.path());
            files = files.saturating_add(f);
            bytes = bytes.saturating_add(b);
        }
    }
    (files, bytes)
}

fn write_pull_results<W: io::Write>(
    out: &mut W,
    results: &[PullResult],
    cache_root: &Path,
    mode: OutputMode,
) -> io::Result<()> {
    match mode {
        OutputMode::Human => {
            if results.is_empty() {
                writeln!(out, "No sources pulled.")?;
                return Ok(());
            }
            writeln!(
                out,
                "{:<20}  {:<8}  {:<10}  {:<6}  PATH",
                "NAME", "FILES", "SIZE", "DRY"
            )?;
            for r in results {
                writeln!(
                    out,
                    "{:<20}  {:<8}  {:<10}  {:<6}  {}",
                    r.name,
                    r.file_count,
                    format_bytes(r.byte_count),
                    if r.dry_run { "yes" } else { "no" },
                    r.data_dir
                )?;
            }
            writeln!(out)?;
            writeln!(out, "Cache: {}", cache_root.display())?;
            Ok(())
        }
        OutputMode::Json => {
            let payload = serde_json::json!({
                "results": results,
                "cache_dir": cache_root.display().to_string(),
            });
            serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
            writeln!(out)
        }
        OutputMode::Ndjson => {
            for r in results {
                serde_json::to_writer(&mut *out, r).map_err(std::io::Error::other)?;
                writeln!(out)?;
            }
            Ok(())
        }
    }
}

#[cfg(all(test, unix))]
mod dir_count_tests {
    use super::super::super::dir_size_bytes;
    use super::count_dir;
    use std::os::unix::fs::symlink;

    #[test]
    fn recursive_dir_accounting_skips_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("real.txt"), "12345").unwrap();
        symlink(root, root.join("loop")).unwrap();
        symlink(root.join("real.txt"), root.join("file-link")).unwrap();

        assert_eq!(dir_size_bytes(root), 5);
        assert_eq!(count_dir(root), (1, 5));
    }
}
