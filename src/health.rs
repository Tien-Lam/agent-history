//! Reusable health checks shared by `aghist health` and the MCP `health` tool.
//!
//! Each check returns a [`HealthCheck`] with a stable `name` (kebab-case),
//! a [`HealthStatus`] (`ok` / `warn` / `fail`), a human message, and an
//! optional remediation hint. Renderers (CLI, MCP) format these as they like
//! but the structure is the contract.

use std::path::Path;

use serde::Serialize;

use crate::provider::HistoryProvider;
use crate::provider_diagnostic::{analyze_provider, ProviderDiagnostic};
use crate::search::SearchIndex;

/// Cap on sessions sampled per provider when computing the live-data
/// fidelity summary. Real session stores can hold thousands of sessions;
/// `aghist health` must stay snappy, so we sample the most recent
/// `discover_sessions` returned (provider-specific ordering).
pub const HEALTH_FIDELITY_SAMPLE_PER_PROVIDER: usize = 5;

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

pub fn run_health_checks(providers: &[Box<dyn HistoryProvider>]) -> Vec<HealthCheck> {
    let mut checks = Vec::new();

    if providers.is_empty() {
        checks.push(HealthCheck {
            name: "providers-detected",
            status: HealthStatus::Warn,
            message: "no providers detected on this system".to_string(),
            hint: Some("Use one of the supported agents (claude-code, copilot-cli, gemini-cli, codex-cli, opencode, cursor), or check `aghist sources`.".to_string()),
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

    checks
}

/// Sample each detected provider for tool-call fidelity. Returns one
/// [`ProviderDiagnostic`] per provider. Errors during discover/load are
/// converted to a synthetic diagnostic with `provider` set to the slug
/// and zero counts so the caller's output remains a stable list.
///
/// Capped at [`HEALTH_FIDELITY_SAMPLE_PER_PROVIDER`] sessions per provider
/// so this never blocks `aghist health` on huge real-world stores.
#[must_use]
pub fn run_provider_fidelity(providers: &[Box<dyn HistoryProvider>]) -> Vec<ProviderDiagnostic> {
    providers
        .iter()
        .map(|p| {
            let slug = p.provider().slug();
            analyze_provider(slug, p.as_ref(), Some(HEALTH_FIDELITY_SAMPLE_PER_PROVIDER))
                .unwrap_or_else(|_| ProviderDiagnostic {
                    label: slug.to_string(),
                    provider: slug.to_string(),
                    session_count: 0,
                    message_count: 0,
                    blocks: crate::provider_diagnostic::BlockCounts::default(),
                    tool_call_fidelity: crate::provider_diagnostic::ToolCallFidelity::default(),
                })
        })
        .collect()
}

fn check_dir_writable(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(".aghist-health-probe");
    std::fs::write(&probe, b"ok")?;
    std::fs::remove_file(&probe)?;
    Ok(())
}
