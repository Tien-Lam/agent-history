use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::config::{self, Config, RemoteSource};
use crate::federated::{self, FederatedDiscovery, SourceFailure};
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
            let mut local = federated::discover_federated(local_providers, &[], Path::new(""));
            local
                .failures
                .extend(self.sources.iter().map(source_cache_failure));
            local
        };
        self.retain_discovery(&mut discovery);
        discovery
    }

    pub fn discover_remote_sources(&self) -> FederatedDiscovery {
        let mut discovery = if let Some(cache_root) = self.sources_cache_root() {
            federated::discover_remote_sources(&self.sources, cache_root)
        } else {
            FederatedDiscovery {
                sessions: Vec::new(),
                source_by_session: HashMap::new(),
                failures: self.sources.iter().map(source_cache_failure).collect(),
            }
        };
        self.retain_discovery(&mut discovery);
        discovery
    }
}

pub fn detect_enabled_providers(config: &Config) -> Vec<Box<dyn HistoryProvider>> {
    QueryScope::enabled(config).filter_provider_instances(provider::detect_all_providers())
}

fn source_cache_failure(source: &RemoteSource) -> SourceFailure {
    SourceFailure {
        source: source.name.clone(),
        message: "sources cache dir unavailable".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::config::{Config, ProviderConfig, RemoteSource, Transport};
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
    fn enabled_scope_uses_validated_provider_set() {
        let config = Config {
            providers: ProviderConfig {
                enabled: vec![
                    Provider::ClaudeCode.slug().to_string(),
                    Provider::CodexCli.slug().to_string(),
                ],
                mcp_exposed: None,
            },
            ..Config::default()
        };

        let scope = QueryScope::enabled(&config);
        assert_eq!(scope.providers().len(), 2);
        assert!(scope.contains_provider(Provider::ClaudeCode));
        assert!(scope.contains_provider(Provider::CodexCli));
    }

    #[test]
    fn discovery_without_source_cache_root_reports_each_remote_source() {
        let scope = QueryScope::from_parts(
            HashSet::from([Provider::ClaudeCode]),
            vec![remote_source("desk"), remote_source("laptop")],
            None,
        );

        let discovery = scope.discover_federated(&[]);

        assert!(discovery.sessions.is_empty());
        let failures: Vec<_> = discovery
            .failures
            .iter()
            .map(|failure| (failure.source.as_str(), failure.message.as_str()))
            .collect();
        assert_eq!(
            failures,
            vec![
                ("desk", "sources cache dir unavailable"),
                ("laptop", "sources cache dir unavailable"),
            ]
        );
    }

    #[test]
    fn remote_discovery_without_source_cache_root_preserves_source_failures() {
        let scope = QueryScope::from_parts(
            HashSet::from([Provider::ClaudeCode]),
            vec![remote_source("desk")],
            None,
        );

        let discovery = scope.discover_remote_sources();

        assert!(discovery.sessions.is_empty());
        assert_eq!(discovery.failures.len(), 1);
        assert_eq!(discovery.failures[0].source, "desk");
        assert_eq!(
            discovery.failures[0].message,
            "sources cache dir unavailable"
        );
    }

    fn remote_source(name: &str) -> RemoteSource {
        RemoteSource {
            name: name.to_string(),
            host: "host.example".to_string(),
            path: "/history".to_string(),
            transport: Transport::Ssh,
        }
    }
}
