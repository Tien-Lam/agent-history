use std::path::Path;

use crate::config::{RemoteSource, SourceCacheManifest};
use crate::health::{HealthCheck, HealthStatus};

pub(in crate::health) fn source_cache_manifest_health_check(
    sources: &[RemoteSource],
    cache_root: Option<&Path>,
) -> HealthCheck {
    if sources.is_empty() {
        return HealthCheck {
            name: "source-cache-manifests",
            status: HealthStatus::Ok,
            message: "no remote source caches to inspect".to_string(),
            hint: None,
        };
    }

    let Some(cache_root) = cache_root else {
        return HealthCheck {
            name: "source-cache-manifests",
            status: HealthStatus::Warn,
            message: "remote sources are registered, but the source cache root is unavailable"
                .to_string(),
            hint: Some("Set AGHIST_SOURCES_CACHE_DIR or restore HOME/XDG cache dirs.".to_string()),
        };
    };

    let mut failures = Vec::new();
    let mut warnings = Vec::new();
    for source in sources {
        let manifest_path = source.manifest_path(cache_root);
        if !manifest_path.exists() {
            warnings.push(format!("{} missing manifest", source.name));
            continue;
        }

        let manifest = match SourceCacheManifest::try_load(&manifest_path) {
            Ok(manifest) => manifest,
            Err(e) => {
                failures.push(format!("{}: {e}", source.name));
                continue;
            }
        };

        if let Some(message) = source_manifest_mismatch(source, &manifest, cache_root) {
            warnings.push(message);
        }

        let data_dir = source.data_dir(cache_root);
        match std::fs::metadata(&data_dir) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => failures.push(format!("{} data path is not a directory", source.name)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warnings.push(format!("{} data dir missing", source.name));
            }
            Err(e) => failures.push(format!("{} data dir unreadable: {e}", source.name)),
        }
    }

    if !failures.is_empty() {
        HealthCheck {
            name: "source-cache-manifests",
            status: HealthStatus::Fail,
            message: format!(
                "remote source cache manifests are corrupt: {}",
                failures.join("; ")
            ),
            hint: Some("Run `aghist sources pull --all` after fixing the cache path.".to_string()),
        }
    } else if !warnings.is_empty() {
        HealthCheck {
            name: "source-cache-manifests",
            status: HealthStatus::Warn,
            message: format!("remote source caches need refresh: {}", warnings.join("; ")),
            hint: Some(
                "Run `aghist sources pull --all` to refresh registered source caches.".to_string(),
            ),
        }
    } else {
        HealthCheck {
            name: "source-cache-manifests",
            status: HealthStatus::Ok,
            message: format!("{} remote source cache manifest(s) readable", sources.len()),
            hint: None,
        }
    }
}

fn source_manifest_mismatch(
    source: &RemoteSource,
    manifest: &SourceCacheManifest,
    cache_root: &Path,
) -> Option<String> {
    let expected_data_dir = source.data_dir(cache_root).display().to_string();
    let mut mismatches = Vec::new();
    if manifest.name != source.name {
        mismatches.push("name");
    }
    if manifest.host != source.host {
        mismatches.push("host");
    }
    if manifest.path != source.path {
        mismatches.push("path");
    }
    if manifest.transport != source.transport {
        mismatches.push("transport");
    }
    if manifest.data_dir != expected_data_dir {
        mismatches.push("data_dir");
    }

    if mismatches.is_empty() {
        None
    } else {
        Some(format!(
            "{} manifest stale ({})",
            source.name,
            mismatches.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests;
