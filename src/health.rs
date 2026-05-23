//! Reusable health checks shared by `aghist health` and the MCP `health` tool.
//!
//! Each check returns a [`HealthCheck`] with a stable `name` (kebab-case),
//! a [`HealthStatus`] (`ok` / `warn` / `fail`), a human message, and an
//! optional remediation hint. Renderers (CLI, MCP) format these as they like
//! but the structure is the contract.

mod fs;
mod index;
mod provider_fidelity;
mod sidecars;

use serde::Serialize;

use crate::model::Provider;
use crate::provider::HistoryProvider;
use crate::query_scope::QueryScope;

pub use provider_fidelity::{
    provider_parse_health_check, run_provider_fidelity, HEALTH_FIDELITY_SAMPLE_PER_PROVIDER,
};

use index::index_health_checks;
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

    let index_dir = crate::search::SearchIndex::default_index_dir();
    checks.extend(index_health_checks(&index_dir));

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
