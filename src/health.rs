//! Reusable health checks shared by `aghist health` and the MCP `health` tool.
//!
//! Each check returns a [`HealthCheck`] with a stable `name` (kebab-case),
//! a [`HealthStatus`] (`ok` / `warn` / `fail`), a human message, and an
//! optional remediation hint. Renderers (CLI, MCP) format these as they like
//! but the structure is the contract.

mod fs;
mod provider_fidelity;
mod sidecars;

use serde::Serialize;

use crate::model::Provider;
use crate::provider::HistoryProvider;
use crate::query_scope::QueryScope;
use crate::search::SearchIndex;

pub use provider_fidelity::{
    provider_parse_health_check, run_provider_fidelity, HEALTH_FIDELITY_SAMPLE_PER_PROVIDER,
};

use fs::check_dir_writable;
use sidecars::{
    embedding_consent_health_check, embedding_store_health_check, metadata_db_health_check,
    source_cache_manifest_health_check, source_registry_health_check,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthCheck {
    pub name: &'static str,
    pub status: HealthStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

pub fn run_health_checks(
    providers: &[Box<dyn HistoryProvider>],
    scope: &QueryScope,
) -> Vec<HealthCheck> {
    let mut checks = Vec::new();

    if providers.is_empty() {
        checks.push(HealthCheck {
            name: "providers-detected",
            status: HealthStatus::Warn,
            message: "no providers detected on this system".to_string(),
            hint: Some(format!(
                "Use one of the supported agents ({}), or check `aghist sources`.",
                supported_provider_slugs()
            )),
        });
    } else {
        let slugs: Vec<&str> = providers.iter().map(|p| p.provider().slug()).collect();
        checks.push(HealthCheck {
            name: "providers-detected",
            status: HealthStatus::Ok,
            message: format!(
                "{} provider(s) detected: {}",
                providers.len(),
                slugs.join(", ")
            ),
            hint: None,
        });
    }

    let index_dir = SearchIndex::default_index_dir();
    match check_dir_writable(&index_dir) {
        Ok(()) => checks.push(HealthCheck {
            name: "index-dir-writable",
            status: HealthStatus::Ok,
            message: format!("index dir writable: {}", index_dir.display()),
            hint: None,
        }),
        Err(e) => checks.push(HealthCheck {
            name: "index-dir-writable",
            status: HealthStatus::Fail,
            message: format!("index dir not writable ({}): {e}", index_dir.display()),
            hint: Some("Set $AGHIST_INDEX_DIR to a writable path, or fix permissions.".to_string()),
        }),
    }

    let manifest_path = index_dir.join("manifest.json");
    if manifest_path.exists() {
        match std::fs::read_to_string(&manifest_path)
            .map_err(|e| e.to_string())
            .and_then(|raw| {
                serde_json::from_str::<serde_json::Value>(&raw).map_err(|e| e.to_string())
            }) {
            Ok(v) if v.get("sessions").is_some() => checks.push(HealthCheck {
                name: "manifest-sane",
                status: HealthStatus::Ok,
                message: "manifest.json parses and has 'sessions' field".to_string(),
                hint: None,
            }),
            Ok(_) => checks.push(HealthCheck {
                name: "manifest-sane",
                status: HealthStatus::Warn,
                message: "manifest.json parses but is missing 'sessions' field".to_string(),
                hint: Some("Run `aghist index --force` to rebuild the manifest.".to_string()),
            }),
            Err(e) => checks.push(HealthCheck {
                name: "manifest-sane",
                status: HealthStatus::Fail,
                message: format!("manifest.json failed to parse: {e}"),
                hint: Some("Run `aghist index --force` to rebuild the manifest.".to_string()),
            }),
        }
    } else {
        checks.push(HealthCheck {
            name: "manifest-sane",
            status: HealthStatus::Warn,
            message: "no manifest.json — index has not been built".to_string(),
            hint: Some("Run `aghist index` to populate the search index.".to_string()),
        });
    }

    let meta_path = index_dir.join("meta.json");
    if meta_path.exists() {
        checks.push(HealthCheck {
            name: "index-schema-present",
            status: HealthStatus::Ok,
            message: "Tantivy meta.json present".to_string(),
            hint: None,
        });
    } else {
        checks.push(HealthCheck {
            name: "index-schema-present",
            status: HealthStatus::Warn,
            message: "Tantivy meta.json missing — index has not been initialised".to_string(),
            hint: Some("Run `aghist index` to create the index.".to_string()),
        });
    }

    checks.push(metadata_db_health_check());
    checks.push(source_registry_health_check(scope.sources()));
    checks.push(source_cache_manifest_health_check(
        scope.sources(),
        scope.sources_cache_root(),
    ));
    let (consent_check, consent_present) = embedding_consent_health_check(&index_dir);
    checks.push(consent_check);
    checks.push(embedding_store_health_check(&index_dir, consent_present));

    checks
}

fn supported_provider_slugs() -> String {
    Provider::all()
        .iter()
        .map(|provider| provider.slug())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn no_provider_hint_tracks_provider_registry() {
        let scope = QueryScope::local(HashSet::new());
        let checks = run_health_checks(&[], &scope);
        let providers_check = checks
            .iter()
            .find(|check| check.name == "providers-detected")
            .expect("providers-detected check");
        let hint = providers_check.hint.as_deref().expect("provider hint");

        for provider in Provider::all() {
            assert!(
                hint.contains(provider.slug()),
                "missing provider slug {} in hint {hint:?}",
                provider.slug()
            );
        }
    }
}
