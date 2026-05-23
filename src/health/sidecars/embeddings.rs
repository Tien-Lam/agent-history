use std::path::Path;

use crate::embed::{Consent, EmbedError, EmbeddingStore};
use crate::health::{HealthCheck, HealthStatus};

pub(in crate::health) fn embedding_consent_health_check(index_dir: &Path) -> (HealthCheck, bool) {
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

pub(in crate::health) fn embedding_store_health_check(
    index_dir: &Path,
    consent_present: bool,
) -> HealthCheck {
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
                    message: format!(
                        "{message}; consent marker is missing so hybrid search is disabled"
                    ),
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
}
