use serde_json::{json, Value};

use super::payload::{message_row_with_source, session_row_with_source};
use super::protocol::{RpcError, ERR_INVALID_PARAMS};
use super::resources::{
    parse_aghist_uri, resource_descriptor_with_source, session_uri_for_source, turn_uri_for_source,
    ParsedUri,
};
use super::server::McpServer;

use crate::federated::LOCAL_SOURCE;
use crate::model::Provider;
use crate::provider;

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
        let resources: Vec<Value> = sessions
            .iter()
            .map(|session| {
                resource_descriptor_with_source(session, discovery.source_of_session(session))
            })
            .collect();
        json!({ "resources": resources })
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
        let located = self.find_session_exact(provider_want, session_id, source)?;
        let messages = provider::load_messages_for_session(&located.session, &self.providers)
            .map_err(|e| format!("failed to load messages: {e}"))?;
        let turns: Vec<Value> = messages
            .iter()
            .enumerate()
            .map(|(i, m)| message_row_with_source(&located.session, m, i + 1, &located.source))
            .collect();
        Ok(json!({
            "uri": session_uri_for_source(&located.source, located.session.provider, &located.session.id.0),
            "session": session_row_with_source(&located.session, &located.source),
            "turns": turns,
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
        let located = self.find_session_exact(provider_want, session_id, source)?;
        let messages = provider::load_messages_for_session(&located.session, &self.providers)
            .map_err(|e| format!("failed to load messages: {e}"))?;
        let total = messages.len();
        let turn_usize = turn as usize;
        if turn_usize == 0 || turn_usize > total {
            return Err(format!(
                "turn {turn} out of range: session has {total} message(s)"
            ));
        }
        let msg = &messages[turn_usize - 1];
        Ok(json!({
            "uri": turn_uri_for_source(&located.source, located.session.provider, &located.session.id.0, turn),
            "session": session_row_with_source(&located.session, &located.source),
            "turn": message_row_with_source(&located.session, msg, turn_usize, &located.source),
        }))
    }
}
