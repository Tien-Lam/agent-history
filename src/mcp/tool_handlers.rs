use serde_json::{json, Value};

use super::args::{optional_provider, optional_str, optional_usize, required_str};
use super::payload::{message_row_with_source, session_row_with_source, tool_error, tool_success};
use super::protocol::{RpcError, ERR_INVALID_PARAMS};
use super::server::McpServer;

use crate::federated::{self, LOCAL_SOURCE};
use crate::health::{run_health_checks, HealthStatus};
use crate::indexing::{self, IndexingOptions, UnfilteredIndexScope};
use crate::model::{QualifiedCitationRef, Session};
use crate::provider;

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
        let limit = optional_usize(args, "limit", 50, 1, 1_000)?;

        let discovery = self.collect_discovery();
        let mut all = discovery.sessions.clone();
        all.sort_by_key(|s| std::cmp::Reverse(s.started_at));

        let filtered: Vec<&Session> = all
            .iter()
            .filter(|s| provider_filter.is_none_or(|want| s.provider == want))
            .filter(|s| {
                project_filter.as_deref().is_none_or(|want| {
                    s.project_name
                        .as_deref()
                        .is_some_and(|got| got.contains(want))
                })
            })
            .take(limit)
            .collect();

        let rows: Vec<Value> = filtered
            .iter()
            .map(|session| session_row_with_source(session, discovery.source_of_session(session)))
            .collect();

        Ok(json!({
            "total": rows.len(),
            "sessions": rows,
            "source_errors": federated::source_errors(&discovery.failures),
        }))
    }

    fn tool_get_session(&self, args: &Value) -> Result<Value, String> {
        let session_id = required_str(args, "session_id")?;
        let provider_filter = optional_provider(args, "provider")?;
        let source_filter = optional_str(args, "source")?;
        let located =
            self.find_session_by_prefix(&session_id, provider_filter, source_filter.as_deref())?;
        let messages = provider::load_messages_for_session(&located.session, &self.providers)
            .map_err(|e| format!("failed to load messages for {}: {e}", located.session.id.0))?;
        let turns: Vec<Value> = messages
            .iter()
            .enumerate()
            .map(|(i, m)| message_row_with_source(&located.session, m, i + 1, &located.source))
            .collect();
        Ok(json!({
            "session": session_row_with_source(&located.session, &located.source),
            "turns": turns,
        }))
    }

    fn tool_get_message(&self, args: &Value) -> Result<Value, String> {
        let raw_ref = required_str(args, "ref")?;
        let qualified: QualifiedCitationRef = raw_ref
            .parse()
            .map_err(|e| format!("invalid ref '{raw_ref}': {e}"))?;
        let include_context = optional_usize(args, "include_context", 0, 0, 100)?;
        let citation = qualified.citation;
        let source = qualified.source.as_deref();

        let located = self.find_session_exact_with_optional_source(
            citation.provider,
            &citation.session_id.0,
            source,
        )?;
        let messages = provider::load_messages_for_session(&located.session, &self.providers)
            .map_err(|e| format!("failed to load messages: {e}"))?;

        let total = messages.len();
        let turn = citation.turn as usize;
        if turn == 0 || turn > total {
            return Err(format!(
                "turn {turn} out of range: session has {total} message(s)"
            ));
        }
        let target_idx = turn - 1;
        let start_idx = target_idx.saturating_sub(include_context);
        let end_idx = (target_idx + include_context + 1).min(total);
        let slice = &messages[start_idx..end_idx];

        let turns: Vec<Value> = slice
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let mut row = message_row_with_source(
                    &located.session,
                    m,
                    start_idx + i + 1,
                    &located.source,
                );
                if let Some(obj) = row.as_object_mut() {
                    obj.insert("is_target".to_string(), json!(start_idx + i == target_idx));
                }
                row
            })
            .collect();

        let response_ref = QualifiedCitationRef::new(
            (located.source != LOCAL_SOURCE).then(|| located.source.clone()),
            citation.clone(),
        )
        .to_string();

        Ok(json!({
            "ref": response_ref,
            "session": session_row_with_source(&located.session, &located.source),
            "target_turn": citation.turn,
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
        let checks = run_health_checks(&self.providers);
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
        })
    }
}
