use serde_json::Value;

use super::super::args::{optional_usize, required_str};
use super::super::server::McpServer;

use crate::dto::McpSearchResponse;
use crate::federated;
use crate::schema_fragments::{MCP_SEARCH_LIMIT_MAX, SEARCH_LIMIT_DEFAULT};
use crate::search::SearchFilters;
use crate::services::search as search_service;

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
        let page = search_service::search_sessions(
            &self.providers,
            &discovery,
            search_service::SearchSessionsRequest {
                query: &query,
                limit,
                cursor: None,
                filters: &filters,
                debug_search: false,
                hybrid_weight: 0.0,
                metadata_keys: None,
                provider_scope: Some(&provider_scope),
            },
        )
        .map_err(|e| e.to_string())?;
        let hits_json = search_service::search_hit_json(&page, &discovery.source_by_session, false);

        let mut source_errors = federated::source_errors(&discovery.failures);
        source_errors.extend(
            page.warnings
                .iter()
                .map(crate::session_warnings::SessionLoadWarning::source_error),
        );

        let response = McpSearchResponse {
            query: query.clone(),
            limit,
            total: hits_json.len(),
            hits: hits_json,
            source_errors,
        };
        serde_json::to_value(response)
            .map_err(|e| format!("failed to serialize search response: {e}"))
    }
}
