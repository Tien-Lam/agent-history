use std::collections::HashSet;
use std::path::Path;

use crate::config::{RemoteSource, SourceCacheManifest};
use crate::health::{HealthCheck, HealthStatus};

pub(in crate::health) fn source_registry_health_check(sources: &[RemoteSource]) -> HealthCheck {
    if sources.is_empty() {
        return HealthCheck {
            name: "source-registry-valid",
            status: HealthStatus::Ok,
            message: "no remote sources registered".to_string(),
            hint: None,
        };
    }

    let mut names = HashSet::new();
    let mut issues = Vec::new();
    for source in sources {
        if !names.insert(source.name.as_str()) {
            issues.push(format!("duplicate source name '{}'", source.name));
        }
        if let Err(message) = source.validate() {
            issues.push(format!("{}: {message}", source.name));
        }
    }

    if issues.is_empty() {
        HealthCheck {
            name: "source-registry-valid",
            status: HealthStatus::Ok,
            message: format!("{} remote source(s) registered and valid", sources.len()),
            hint: None,
        }
    } else {
        HealthCheck {
            name: "source-registry-valid",
            status: HealthStatus::Fail,
            message: format!("remote source registry has invalid entries: {}", issues.join("; ")),
            hint: Some(
                "Fix the [[sources]] entries in config.toml, or re-create them with `aghist sources add`."
                    .to_string(),
            ),
        }
    }
}

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
mod tests {
    use super::*;
    use crate::config::Transport;

    #[test]
    fn source_registry_health_check_rejects_invalid_and_duplicate_sources() {
        let sources = vec![
            test_source("box", "host", "/history"),
            test_source("box", "host2", "/other"),
            test_source("../escape", "host", "/history"),
        ];

        let check = source_registry_health_check(&sources);

        assert_eq!(check.status, HealthStatus::Fail);
        assert!(check.message.contains("duplicate source name 'box'"));
        assert!(check.message.contains("../escape"));
    }

    #[test]
    fn source_cache_manifest_health_check_fails_corrupt_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let source = test_source("box", "host", "/history");
        let manifest_path = source.manifest_path(dir.path());
        std::fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        std::fs::write(&manifest_path, "{not-json").unwrap();

        let check = source_cache_manifest_health_check(&[source], Some(dir.path()));

        assert_eq!(check.status, HealthStatus::Fail);
        assert!(check.message.contains("failed to parse"));
    }

    #[test]
    fn source_cache_manifest_health_check_warns_on_stale_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let source = test_source("box", "host", "/history");
        let data_dir = source.data_dir(dir.path());
        std::fs::create_dir_all(&data_dir).unwrap();
        SourceCacheManifest {
            name: source.name.clone(),
            host: "old-host".to_string(),
            path: source.path.clone(),
            transport: source.transport,
            data_dir: data_dir.display().to_string(),
            last_pulled_at: chrono::Utc::now(),
            last_pull_dry_run: false,
            byte_count: 0,
            file_count: 0,
        }
        .save(&source.manifest_path(dir.path()))
        .unwrap();

        let check = source_cache_manifest_health_check(&[source], Some(dir.path()));

        assert_eq!(check.status, HealthStatus::Warn);
        assert!(check.message.contains("manifest stale"));
    }

    fn test_source(name: &str, host: &str, path: &str) -> RemoteSource {
        RemoteSource {
            name: name.to_string(),
            host: host.to_string(),
            path: path.to_string(),
            transport: Transport::Ssh,
        }
    }
}
