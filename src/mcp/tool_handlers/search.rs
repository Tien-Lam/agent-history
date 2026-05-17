use serde_json::Value;

use super::super::args::{optional_usize, required_str};
use super::super::server::McpServer;

use crate::dto::{McpSearchResponse, SearchHitJson};
use crate::federated;
use crate::schema_fragments::{MCP_SEARCH_LIMIT_MAX, SEARCH_LIMIT_DEFAULT};
use crate::search::{SearchFilters, SearchService, SearchServiceOutput, SearchServiceRequest};

impl McpServer {
    pub(super) fn tool_search_sessions(&self, args: &Value) -> Result<Value, String> {
        let query = required_str(args, "query")?;
        if query.trim().is_empty() {
            return Err("query is empty".to_string());
        }
        let limit = optional_usize(args, "limit", SEARCH_LIMIT_DEFAULT, 1, MCP_SEARCH_LIMIT_MAX)?;

        let discovery = self.collect_discovery();
        let provider_scope = self.provider_scope();
        let filters = SearchFilters::default();
        let SearchServiceOutput {
            hits, session_meta, ..
        } = SearchService::new(&self.providers)
            .search(
                &discovery.sessions,
                &discovery.source_by_session,
                SearchServiceRequest {
                    query: &query,
                    limit,
                    filters: &filters,
                    debug_search: false,
                    hybrid_weight: 0.0,
                    metadata_keys: None,
                    provider_scope: Some(&provider_scope),
                },
            )
            .map_err(|e| e.to_string())?;

        let hits: Vec<_> = hits.into_iter().take(limit).collect();
        let citations = crate::search::resolve_search_hit_citations(
            &hits,
            &session_meta,
            &discovery.source_by_session,
            &self.providers,
        );

        let mut hits_json = Vec::with_capacity(hits.len());
        for (h, _explanation) in &hits {
            hits_json.push(SearchHitJson::from_search_hit(
                h,
                None,
                &session_meta,
                &discovery.source_by_session,
                Some(&citations),
            ));
        }

        let response = McpSearchResponse {
            query: query.clone(),
            limit,
            total: hits_json.len(),
            hits: hits_json,
            source_errors: federated::source_errors(&discovery.failures),
        };
        serde_json::to_value(response)
            .map_err(|e| format!("failed to serialize search response: {e}"))
    }
}
