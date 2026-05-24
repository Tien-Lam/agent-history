use serde_json::{json, Value};

use super::super::args::{
    optional_provider, optional_str_with_limit, optional_usize, required_str_with_limit,
};
use super::super::payload::{message_row_with_source, session_row_with_source};
use super::super::resources::session_uri_for_source;
use super::super::server::McpServer;

use crate::dto::{McpListResponse, McpSessionRow};
use crate::federated;
use crate::model::QualifiedCitationRef;
use crate::schema_fragments::{
    MCP_FILTER_STRING_MAX_BYTES, MCP_INCLUDE_CONTEXT_DEFAULT, MCP_INCLUDE_CONTEXT_MAX,
    MCP_LIST_LIMIT_DEFAULT, MCP_LIST_LIMIT_MAX, MCP_LOOKUP_STRING_MAX_BYTES,
};
use crate::search::SearchFilters;
use crate::services::list as list_service;
use crate::services::lookup as lookup_service;
use crate::session_resolver::LookupSource;

impl McpServer {
    pub(super) fn tool_list_sessions(&self, args: &Value) -> Result<Value, String> {
        let provider_filter = optional_provider(args, "provider")?;
        let project_filter = optional_str_with_limit(args, "project", MCP_FILTER_STRING_MAX_BYTES)?;
        let limit = optional_usize(args, "limit", MCP_LIST_LIMIT_DEFAULT, 1, MCP_LIST_LIMIT_MAX)?;

        let discovery = self.collect_discovery();
        let source_errors = federated::source_errors(&discovery.failures);
        let filters = SearchFilters {
            provider: provider_filter,
            project: project_filter.clone(),
            ..SearchFilters::default()
        };
        let page = list_service::list_sessions_page(
            &self.providers,
            discovery,
            list_service::ListSessionsRequest {
                limit,
                cursor: None,
                filters: &filters,
                metadata_keys: None,
            },
        )
        .map_err(|e| e.to_string())?;

        let rows: Vec<McpSessionRow> = page
            .sessions
            .iter()
            .map(|listed| {
                McpSessionRow::from_session(
                    &listed.session,
                    &listed.source,
                    session_uri_for_source(
                        &listed.source,
                        listed.session.provider,
                        &listed.session.id.0,
                    ),
                )
            })
            .collect();

        let response = McpListResponse {
            total: page.total,
            sessions: rows,
            source_errors,
        };
        serde_json::to_value(response)
            .map_err(|e| format!("failed to serialize list response: {e}"))
    }

    pub(super) fn tool_get_session(&self, args: &Value) -> Result<Value, String> {
        let session_id = required_str_with_limit(args, "session_id", MCP_LOOKUP_STRING_MAX_BYTES)?;
        let provider_filter = optional_provider(args, "provider")?;
        let source_filter =
            optional_str_with_limit(args, "source", crate::config::MAX_SOURCE_NAME_BYTES)?;
        let discovery = self.collect_discovery();
        let provider_scope = self.provider_scope();
        let loaded = lookup_service::load_session_by_prefix(
            &self.providers,
            &discovery,
            &session_id,
            provider_filter,
            source_filter.as_deref(),
            Some(&provider_scope),
        )
        .map_err(|e| e.message)?;
        let turns: Vec<Value> = loaded
            .messages
            .iter()
            .enumerate()
            .map(|(i, m)| message_row_with_source(&loaded.session, m, i + 1, &loaded.source))
            .collect();
        Ok(json!({
            "session": session_row_with_source(&loaded.session, &loaded.source),
            "turns": turns,
        }))
    }

    pub(super) fn tool_get_message(&self, args: &Value) -> Result<Value, String> {
        let raw_ref = required_str_with_limit(args, "ref", MCP_LOOKUP_STRING_MAX_BYTES)?;
        let qualified: QualifiedCitationRef = raw_ref
            .parse()
            .map_err(|e| format!("invalid ref '{raw_ref}': {e}"))?;
        let include_context = optional_usize(
            args,
            "include_context",
            MCP_INCLUDE_CONTEXT_DEFAULT,
            0,
            MCP_INCLUDE_CONTEXT_MAX,
        )?;
        let citation = qualified.citation;
        let source =
            LookupSource::from_optional(qualified.source.as_deref()).map_err(|e| e.to_string())?;
        let discovery = self.collect_discovery();
        let provider_scope = self.provider_scope();
        let loaded = lookup_service::load_exact_citation_window(
            &self.providers,
            &discovery,
            citation,
            source,
            include_context,
            Some(&provider_scope),
        )
        .map_err(|e| e.message)?;

        let turns: Vec<Value> = loaded
            .messages
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let mut row = message_row_with_source(
                    &loaded.session,
                    m,
                    loaded.start_idx + i + 1,
                    &loaded.source,
                );
                if let Some(obj) = row.as_object_mut() {
                    obj.insert(
                        "is_target".to_string(),
                        json!(loaded.start_idx + i == loaded.target_idx),
                    );
                }
                row
            })
            .collect();

        Ok(json!({
            "ref": loaded.citation_ref,
            "session": session_row_with_source(&loaded.session, &loaded.source),
            "target_turn": loaded.citation.turn,
            "turns": turns,
        }))
    }
}
