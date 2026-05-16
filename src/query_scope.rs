use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::config::{self, Config, RemoteSource};
use crate::federated::{self, FederatedDiscovery};
use crate::model::Provider;
use crate::provider::{self, HistoryProvider};

/// Provider and remote-source visibility rules for a command surface.
#[derive(Clone, Debug)]
pub struct QueryScope {
    providers: HashSet<Provider>,
    sources: Vec<RemoteSource>,
    sources_cache_root: Option<PathBuf>,
}

impl QueryScope {
    pub fn enabled(config: &Config) -> Self {
        Self::from_config(config, config.enabled_providers())
    }

    pub fn mcp_visible(config: &Config) -> Self {
        Self::from_config(config, config.mcp_exposed_providers())
    }

    pub fn local(providers: HashSet<Provider>) -> Self {
        Self {
            providers,
            sources: Vec::new(),
            sources_cache_root: None,
        }
    }

    pub fn from_parts(
        providers: HashSet<Provider>,
        sources: Vec<RemoteSource>,
        sources_cache_root: Option<PathBuf>,
    ) -> Self {
        Self {
            providers,
            sources,
            sources_cache_root,
        }
    }

    fn from_config(config: &Config, providers: HashSet<Provider>) -> Self {
        Self {
            providers,
            sources: config.sources.clone(),
            sources_cache_root: config::sources_cache_root(),
        }
    }

    pub fn providers(&self) -> &HashSet<Provider> {
        &self.providers
    }

    pub fn contains_provider(&self, provider: Provider) -> bool {
        self.providers.contains(&provider)
    }

    pub fn sources(&self) -> &[RemoteSource] {
        &self.sources
    }

    pub fn has_remote_sources(&self) -> bool {
        !self.sources.is_empty()
    }

    pub fn sources_cache_root(&self) -> Option<&Path> {
        self.sources_cache_root.as_deref()
    }

    pub fn filter_provider_instances(
        &self,
        providers: Vec<Box<dyn HistoryProvider>>,
    ) -> Vec<Box<dyn HistoryProvider>> {
        providers
            .into_iter()
            .filter(|provider| self.contains_provider(provider.provider()))
            .collect()
    }

    pub fn retain_discovery(&self, discovery: &mut FederatedDiscovery) {
        discovery.retain_providers(&self.providers);
    }

    pub fn discover_federated(
        &self,
        local_providers: &[Box<dyn HistoryProvider>],
    ) -> FederatedDiscovery {
        let mut discovery = if let Some(cache_root) = self.sources_cache_root() {
            federated::discover_federated(local_providers, &self.sources, cache_root)
        } else {
            federated::discover_federated(local_providers, &[], Path::new(""))
        };
        self.retain_discovery(&mut discovery);
        discovery
    }

    pub fn discover_remote_sources(&self) -> Option<FederatedDiscovery> {
        let cache_root = self.sources_cache_root()?;
        let mut discovery = federated::discover_remote_sources(&self.sources, cache_root);
        self.retain_discovery(&mut discovery);
        Some(discovery)
    }
}

pub fn detect_enabled_providers(config: &Config) -> Vec<Box<dyn HistoryProvider>> {
    QueryScope::enabled(config).filter_provider_instances(provider::detect_all_providers())
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, ProviderConfig};
    use crate::model::Provider;

    use super::QueryScope;

    #[test]
    fn mcp_visible_scope_is_narrowing_only() {
        let config = Config {
            providers: ProviderConfig {
                enabled: vec![
                    Provider::ClaudeCode.slug().to_string(),
                    Provider::GeminiCli.slug().to_string(),
                ],
                mcp_exposed: Some(vec![
                    Provider::ClaudeCode.slug().to_string(),
                    Provider::CopilotCli.slug().to_string(),
                ]),
            },
            ..Config::default()
        };

        let scope = QueryScope::mcp_visible(&config);
        assert!(scope.contains_provider(Provider::ClaudeCode));
        assert!(!scope.contains_provider(Provider::CopilotCli));
        assert!(!scope.contains_provider(Provider::GeminiCli));
    }

    #[test]
    fn enabled_scope_ignores_unknown_provider_slugs() {
        let config = Config {
            providers: ProviderConfig {
                enabled: vec![
                    Provider::ClaudeCode.slug().to_string(),
                    "made-up-provider".to_string(),
                ],
                mcp_exposed: None,
            },
            ..Config::default()
        };

        let scope = QueryScope::enabled(&config);
        assert_eq!(scope.providers().len(), 1);
        assert!(scope.contains_provider(Provider::ClaudeCode));
    }
}
