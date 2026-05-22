use serde_json::{json, Value};

use super::args::{optional_provider, optional_str, optional_usize, required_str};
use super::payload::{message_row_with_source, session_row_with_source, tool_error, tool_success};
use super::protocol::{RpcError, ERR_INVALID_PARAMS};
use super::resources::session_uri_for_source;
use super::server::McpServer;

use crate::dto::{McpListResponse, McpSessionRow};
use crate::federated;
use crate::health::{
    provider_parse_health_check, run_health_checks, run_provider_fidelity, HealthStatus,
};
use crate::indexing::{self, IndexingOptions, UnfilteredIndexScope};
use crate::model::QualifiedCitationRef;
use crate::schema_fragments::{
    MCP_INCLUDE_CONTEXT_DEFAULT, MCP_INCLUDE_CONTEXT_MAX, MCP_LIST_LIMIT_DEFAULT,
    MCP_LIST_LIMIT_MAX,
};
use crate::search::SearchFilters;
use crate::services::list as list_service;
use crate::services::lookup as lookup_service;
use crate::session_resolver::LookupSource;

mod search;

impl McpServer {
    pub(super) fn tools_call(&self, params: &Value) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(ERR_INVALID_PARAMS, "missing 'name' field"))?;
        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

        // Tool-level errors are reported in MCP's content envelope
        // (isError=true), not as JSON-RPC errors, so the model can read the
        // message.
        let outcome = match name {
            "search_sessions" => self.tool_search_sessions(&arguments),
            "list_sessions" => self.tool_list_sessions(&arguments),
            "get_session" => self.tool_get_session(&arguments),
            "get_message" => self.tool_get_message(&arguments),
            "reindex" => self.tool_reindex(&arguments),
            "health" => Ok(self.tool_health(&arguments)),
            other => {
                return Ok(tool_error(format!("unknown tool: {other}")));
            }
        };

        match outcome {
            Ok(payload) => Ok(tool_success(&payload)),
            Err(msg) => Ok(tool_error(msg)),
        }
    }

    fn tool_list_sessions(&self, args: &Value) -> Result<Value, String> {
        let provider_filter = optional_provider(args, "provider")?;
        let project_filter = optional_str(args, "project")?;
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

    fn tool_get_session(&self, args: &Value) -> Result<Value, String> {
        let session_id = required_str(args, "session_id")?;
        let provider_filter = optional_provider(args, "provider")?;
        let source_filter = optional_str(args, "source")?;
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

    fn tool_get_message(&self, args: &Value) -> Result<Value, String> {
        let raw_ref = required_str(args, "ref")?;
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

    fn tool_reindex(&self, args: &Value) -> Result<Value, String> {
        let provider_filter = optional_provider(args, "provider")?;
        let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
        let provider_scope = self.provider_scope();

        if let Some(want) = provider_filter {
            if !provider_scope.contains(&want) {
                return Err(format!("provider '{}' is not visible to MCP", want.slug()));
            }
        }

        let outcome = indexing::run_indexing(
            &self.providers,
            self.scope(),
            IndexingOptions {
                provider_filter,
                force,
                unfiltered_scope: UnfilteredIndexScope::VisibleProviders,
            },
        )
        .map_err(|e| e.message)?;

        serde_json::to_value(outcome.summary)
            .map_err(|e| format!("failed to serialize reindex summary: {e}"))
    }

    fn tool_health(&self, _args: &Value) -> Value {
        let fidelity = run_provider_fidelity(&self.providers);
        let mut checks = run_health_checks(&self.providers);
        if let Some(check) = provider_parse_health_check(&fidelity) {
            checks.push(check);
        }
        let any_failed = checks.iter().any(|c| c.status == HealthStatus::Fail);
        let summary = json!({
            "ok_count": checks.iter().filter(|c| c.status == HealthStatus::Ok).count(),
            "warn_count": checks.iter().filter(|c| c.status == HealthStatus::Warn).count(),
            "fail_count": checks.iter().filter(|c| c.status == HealthStatus::Fail).count(),
        });
        json!({
            "ok": !any_failed,
            "checks": checks,
            "summary": summary,
            "provider_fidelity": fidelity,
        })
    }
}
