use std::io::{self, Write as _};
use std::path::Path;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::config;
use aghist::output::OutputMode;
use chrono::Utc;

use super::{load_sources_config, resolve_config_path};
use output::{write_pull_results, PullResult};
use rsync::run_rsync_pull;
use safety::{count_dir, ensure_cache_dir, ensure_cache_root_safe, resolve_sources_cache_root};

mod output;
mod rsync;
mod safety;

pub(crate) fn sources_pull_remote(
    name: Option<&str>,
    all: bool,
    dry_run: bool,
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    let config_path = resolve_config_path()?;
    let config = load_sources_config(&config_path)?;

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
    ensure_cache_root_safe(&cache_root)?;
    let mut results = Vec::with_capacity(targets.len());
    for src in targets {
        let result = pull_one_source(&src, &cache_root, dry_run)?;
        results.push(result);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_pull_results(&mut out, &results, &cache_root, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write pull output", e))?;
    out.flush()
        .map_err(|e| ErrorEnvelope::io("failed to flush pull output", e))?;
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
    let data_dir = src.data_dir(cache_root);

    let dry_run_target = if dry_run {
        Some(tempfile::tempdir().map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to create dry-run cache dir: {e}"),
            )
        })?)
    } else {
        ensure_cache_dir(&source_dir, "source cache dir")?;
        ensure_cache_dir(&data_dir, "source data dir")?;
        None
    };
    let rsync_data_dir = dry_run_target
        .as_ref()
        .map_or(data_dir.as_path(), |dir| dir.path());

    run_rsync_pull(src, rsync_data_dir, dry_run)?;

    let pulled_at = Utc::now();
    if dry_run {
        return Ok(PullResult {
            name: src.name.clone(),
            host: src.host.clone(),
            path: src.path.clone(),
            transport: src.transport.slug().to_string(),
            data_dir: data_dir.display().to_string(),
            dry_run,
            byte_count: 0,
            file_count: 0,
            pulled_at,
        });
    }

    let (file_count, byte_count) = count_dir(&data_dir);
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
