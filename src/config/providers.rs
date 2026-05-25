use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::Provider;

use super::ConfigLoadError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub enabled: Vec<String>,
    /// Per-provider allowlist for the `aghist mcp` server. When `None`, all
    /// `enabled` providers are visible to MCP clients. When `Some`, only the
    /// intersection of `mcp_exposed` and `enabled` is exposed — letting users
    /// hide history (e.g. a personal Claude account) from agents that don't
    /// need it without disabling the provider for the local TUI/CLI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_exposed: Option<Vec<String>>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            enabled: Provider::all()
                .iter()
                .map(|p| p.slug().to_string())
                .collect(),
            mcp_exposed: None,
        }
    }
}

impl ProviderConfig {
    pub(super) fn validate_slugs(&self, path: &Path) -> Result<(), ConfigLoadError> {
        validate_provider_slug_list(path, "providers.enabled", &self.enabled)?;
        if let Some(exposed) = self.mcp_exposed.as_ref() {
            validate_provider_slug_list(path, "providers.mcp_exposed", exposed)?;
        }
        Ok(())
    }

    pub(super) fn enabled_set(&self) -> HashSet<Provider> {
        self.enabled
            .iter()
            .filter_map(|s| Provider::from_slug(s))
            .collect()
    }

    pub(super) fn mcp_exposed_set(&self) -> HashSet<Provider> {
        let enabled = self.enabled_set();
        let Some(allow) = self.mcp_exposed.as_ref() else {
            return enabled;
        };
        allow
            .iter()
            .filter_map(|s| Provider::from_slug(s))
            .filter(|p| enabled.contains(p))
            .collect()
    }
}

fn validate_provider_slug_list(
    path: &Path,
    field: &'static str,
    slugs: &[String],
) -> Result<(), ConfigLoadError> {
    if let Some(slug) = slugs
        .iter()
        .find(|slug| Provider::from_slug(slug.as_str()).is_none())
    {
        return Err(ConfigLoadError::UnknownProviderSlug {
            path: path.to_path_buf(),
            field,
            slug: slug.clone(),
            expected: expected_provider_slugs(),
        });
    }
    Ok(())
}

fn expected_provider_slugs() -> String {
    Provider::all()
        .iter()
        .map(|provider| provider.slug())
        .collect::<Vec<_>>()
        .join(", ")
}
