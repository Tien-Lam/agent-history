use std::collections::HashSet;

use super::McpServer;
use crate::federated::{self, FederatedDiscovery, SourceFailure, LOCAL_SOURCE};
use crate::model::Provider;
use crate::query_scope::QueryScope;

impl McpServer {
    pub(in crate::mcp) fn collect_discovery(&self) -> FederatedDiscovery {
        let mut discovery = if let Some(cache_root) = self.scope.sources_cache_root() {
            federated::discover_federated(&self.providers, self.scope.sources(), cache_root)
        } else {
            let mut local = self.collect_local_discovery();
            if self.scope.has_remote_sources() {
                local.failures.push(SourceFailure {
                    source: LOCAL_SOURCE.to_string(),
                    message: "sources cache dir unavailable".to_string(),
                });
            }
            local
        };
        self.scope.retain_discovery(&mut discovery);
        discovery
    }

    fn collect_local_discovery(&self) -> FederatedDiscovery {
        let mut all = Vec::new();
        for p in &self.providers {
            if let Ok(found) = p.discover_sessions() {
                all.extend(found);
            }
        }
        let source_by_session = all
            .iter()
            .map(|session| (session.identity_key(), LOCAL_SOURCE.to_string()))
            .collect();
        FederatedDiscovery {
            sessions: all,
            source_by_session,
            failures: Vec::new(),
        }
    }

    pub(in crate::mcp) fn provider_scope(&self) -> HashSet<Provider> {
        self.scope.providers().clone()
    }

    pub(in crate::mcp) fn scope(&self) -> &QueryScope {
        &self.scope
    }
}
