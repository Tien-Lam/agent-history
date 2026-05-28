use serde_json::{json, Value};

use super::payload::{message_row_with_source, session_row_with_source};
use super::protocol::{RpcError, ERR_INVALID_PARAMS};
use super::resources::{
    parse_aghist_uri, resource_descriptor_with_source, session_uri_for_source, turn_uri_for_source,
    ParsedUri,
};
use super::server::McpServer;

use crate::federated::LOCAL_SOURCE;
use crate::model::{CitationRef, Provider, SessionId};
use crate::schema_fragments::{MCP_RESOURCES_LIST_MAX, MCP_SESSION_TURNS_MAX};
use crate::services::lookup as lookup_service;
use crate::session_resolver::LookupSource;

impl McpServer {
    /// Lists every discoverable session as a top-level
    /// `aghist://session/<provider>/<id>` resource. Per-turn URIs are
    /// advertised via the resource template (see `resources/templates/list`)
    /// rather than enumerated, since the turn count would balloon the listing
    /// for large histories.
    pub(super) fn resources_list(&self, _params: &Value) -> Value {
        let discovery = self.collect_discovery();
        let mut sessions = discovery.sessions.clone();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.started_at));
        let total = sessions.len();
        let resources: Vec<Value> = sessions
            .iter()
            .take(MCP_RESOURCES_LIST_MAX)
            .map(|session| {
                resource_descriptor_with_source(session, discovery.source_of_session(session))
            })
            .collect();
        let returned = resources.len();
        json!({
            "resources": resources,
            "meta": {
                "total": total,
                "returned": returned,
                "limit": MCP_RESOURCES_LIST_MAX,
                "truncated": returned < total,
            }
        })
    }

    pub(super) fn resources_read(&self, params: &Value) -> Result<Value, RpcError> {
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(ERR_INVALID_PARAMS, "missing 'uri' field"))?
            .to_string();
        let parsed = parse_aghist_uri(&uri)
            .map_err(|e| RpcError::new(ERR_INVALID_PARAMS, format!("invalid uri '{uri}': {e}")))?;

        let payload = match parsed {
            ParsedUri::Session {
                source,
                provider,
                session_id,
            } => self.read_session_resource(source.as_deref(), provider, &session_id),
            ParsedUri::Turn {
                source,
                provider,
                session_id,
                turn,
            } => self.read_turn_resource(source.as_deref(), provider, &session_id, turn),
        }
        .map_err(|e| RpcError::new(ERR_INVALID_PARAMS, e))?;

        let text = serde_json::to_string_pretty(&payload)
            .unwrap_or_else(|_| "<unserializable>".to_string());
        Ok(json!({
            "contents": [{
                "uri": uri,
                "mimeType": "application/json",
                "text": text,
            }]
        }))
    }

    fn read_session_resource(
        &self,
        source: Option<&str>,
        provider_want: Provider,
        session_id: &str,
    ) -> Result<Value, String> {
        let source = source.unwrap_or(LOCAL_SOURCE);
        let source = LookupSource::explicit(source).map_err(|e| e.to_string())?;
        let discovery = self.collect_discovery();
        let provider_scope = self.provider_scope();
        let loaded = lookup_service::load_exact_session(
            &self.providers,
            &discovery,
            provider_want,
            session_id,
            source,
            Some(&provider_scope),
        )
        .map_err(|e| e.message)?;
        let turns: Vec<Value> = loaded
            .messages
            .iter()
            .take(MCP_SESSION_TURNS_MAX)
            .enumerate()
            .map(|(i, m)| message_row_with_source(&loaded.session, m, i + 1, &loaded.source))
            .collect();
        let turns_total = loaded.messages.len();
        let turns_returned = turns.len();
        Ok(json!({
            "uri": session_uri_for_source(&loaded.source, loaded.session.provider, &loaded.session.id.0),
            "session": session_row_with_source(&loaded.session, &loaded.source),
            "turns": turns,
            "meta": {
                "turns_total": turns_total,
                "turns_returned": turns_returned,
                "turn_limit": MCP_SESSION_TURNS_MAX,
                "truncated": turns_returned < turns_total,
            },
        }))
    }

    fn read_turn_resource(
        &self,
        source: Option<&str>,
        provider_want: Provider,
        session_id: &str,
        turn: u32,
    ) -> Result<Value, String> {
        let source = source.unwrap_or(LOCAL_SOURCE);
        let source = LookupSource::explicit(source).map_err(|e| e.to_string())?;
        let citation = CitationRef::new(provider_want, SessionId(session_id.to_string()), turn)
            .ok_or_else(|| "invalid session id or turn".to_string())?;
        let discovery = self.collect_discovery();
        let provider_scope = self.provider_scope();
        let loaded = lookup_service::load_exact_citation_window(
            &self.providers,
            &discovery,
            citation,
            source,
            0,
            Some(&provider_scope),
        )
        .map_err(|e| e.message)?;
        let msg = loaded
            .messages
            .first()
            .ok_or_else(|| "turn window unexpectedly empty".to_string())?;
        Ok(json!({
            "uri": turn_uri_for_source(&loaded.source, loaded.session.provider, &loaded.session.id.0, turn),
            "session": session_row_with_source(&loaded.session, &loaded.source),
            "turn": message_row_with_source(&loaded.session, msg, turn as usize, &loaded.source),
        }))
    }
}
