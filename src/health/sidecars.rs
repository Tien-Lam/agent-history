use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::config::{RemoteSource, SourceCacheManifest};
use crate::embed::{Consent, EmbedError, EmbeddingStore};
use crate::metadata;

use super::{HealthCheck, HealthStatus};

pub(super) fn metadata_db_health_check() -> HealthCheck {
    metadata_db_health_check_for_path(metadata::default_path())
}

fn metadata_db_health_check_for_path(path: Option<PathBuf>) -> HealthCheck {
    let Some(path) = path else {
        return HealthCheck {
            name: "metadata-db-readable",
            status: HealthStatus::Warn,
            message: "metadata db path could not be resolved".to_string(),
            hint: Some(
                "Set AGHIST_METADATA_DB=/path/to/metadata.db or restore HOME/XDG dirs.".to_string(),
            ),
        };
    };

    if !path.exists() {
        return HealthCheck {
            name: "metadata-db-readable",
            status: HealthStatus::Ok,
            message: format!(
                "metadata db absent; will be created on first write: {}",
                path.display()
            ),
            hint: None,
        };
    }

    match metadata::open(&path) {
        Ok(_) => HealthCheck {
            name: "metadata-db-readable",
            status: HealthStatus::Ok,
            message: format!("metadata db readable: {}", path.display()),
            hint: None,
        },
        Err(e) => HealthCheck {
            name: "metadata-db-readable",
            status: HealthStatus::Fail,
            message: format!("metadata db is not readable ({}): {e}", path.display()),
            hint: Some(
                "Back up the file, then repair it with sqlite tooling or set AGHIST_METADATA_DB to a known-good database."
                    .to_string(),
            ),
        },
    }
}

pub(super) fn source_registry_health_check(sources: &[RemoteSource]) -> HealthCheck {
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

pub(super) fn source_cache_manifest_health_check(
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

pub(super) fn embedding_consent_health_check(index_dir: &Path) -> (HealthCheck, bool) {
    let path = Consent::path(index_dir);
    if !path.exists() {
        return (
            HealthCheck {
                name: "embedding-consent-readable",
                status: HealthStatus::Ok,
                message: "embedding consent absent; semantic indexing is opt-in".to_string(),
                hint: None,
            },
            false,
        );
    }

    let consent = std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|raw| serde_json::from_str::<Consent>(&raw).map_err(|e| e.to_string()));
    match consent {
        Ok(consent) => (
            HealthCheck {
                name: "embedding-consent-readable",
                status: HealthStatus::Ok,
                message: format!(
                    "embedding consent readable for model {} at {}",
                    consent.model,
                    path.display()
                ),
                hint: None,
            },
            true,
        ),
        Err(e) => (
            HealthCheck {
                name: "embedding-consent-readable",
                status: HealthStatus::Fail,
                message: format!(
                    "embedding consent is not readable ({}): {e}",
                    path.display()
                ),
                hint: Some(
                    "Delete embeddings-consent.json or re-run `aghist index --accept-download`."
                        .to_string(),
                ),
            },
            false,
        ),
    }
}

pub(super) fn embedding_store_health_check(index_dir: &Path, consent_present: bool) -> HealthCheck {
    match EmbeddingStore::open(index_dir) {
        Ok(Some(store)) => {
            let message = format!(
                "embedding store readable: model={} dim={} entries={}",
                store.model(),
                store.dim(),
                store.len()
            );
            if consent_present {
                HealthCheck {
                    name: "embedding-store-readable",
                    status: HealthStatus::Ok,
                    message,
                    hint: None,
                }
            } else {
                HealthCheck {
                    name: "embedding-store-readable",
                    status: HealthStatus::Warn,
                    message: format!("{message}; consent marker is missing so hybrid search is disabled"),
                    hint: Some(
                        "Re-run `aghist index --accept-download` to restore semantic-search consent, or delete embeddings.bin."
                            .to_string(),
                    ),
                }
            }
        }
        Ok(None) => {
            if consent_present {
                HealthCheck {
                    name: "embedding-store-readable",
                    status: HealthStatus::Warn,
                    message: "embedding consent exists but embeddings.bin is missing".to_string(),
                    hint: Some("Run `aghist index` to rebuild the embedding sidecar.".to_string()),
                }
            } else {
                HealthCheck {
                    name: "embedding-store-readable",
                    status: HealthStatus::Ok,
                    message: "embedding store absent; lexical search remains available".to_string(),
                    hint: None,
                }
            }
        }
        Err(EmbedError::SchemaMismatch { stored, expected }) => HealthCheck {
            name: "embedding-store-readable",
            status: HealthStatus::Warn,
            message: format!(
                "embedding store schema is v{stored}, current reader expects v{expected}"
            ),
            hint: Some("Run `aghist index` to evict and rebuild the embedding sidecar.".to_string()),
        },
        Err(e) => HealthCheck {
            name: "embedding-store-readable",
            status: HealthStatus::Fail,
            message: format!("embedding store is not readable: {e}"),
            hint: Some("Delete embeddings.bin or run `aghist index --force` to rebuild derived search state.".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RemoteSource, Transport};

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

    #[test]
    fn metadata_db_health_check_reports_corrupt_existing_db() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("metadata.db");
        std::fs::write(&db, "not sqlite").unwrap();

        let check = metadata_db_health_check_for_path(Some(db));

        assert_eq!(check.status, HealthStatus::Fail);
        assert!(check.message.contains("metadata db is not readable"));
    }

    #[test]
    fn embedding_health_checks_warn_when_consent_exists_without_store() {
        let dir = tempfile::tempdir().unwrap();
        Consent::record(dir.path(), crate::embed::DEFAULT_MODEL).unwrap();

        let (consent, present) = embedding_consent_health_check(dir.path());
        let store = embedding_store_health_check(dir.path(), present);

        assert_eq!(consent.status, HealthStatus::Ok);
        assert_eq!(store.status, HealthStatus::Warn);
        assert!(store.message.contains("embeddings.bin is missing"));
    }

    #[test]
    fn embedding_store_health_check_fails_corrupt_store() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("embeddings.bin"), b"not an embedding store").unwrap();

        let check = embedding_store_health_check(dir.path(), false);

        assert_eq!(check.status, HealthStatus::Fail);
        assert!(check.message.contains("embedding store is not readable"));
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
