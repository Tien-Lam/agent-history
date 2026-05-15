use serde_json::{json, Value};

use super::payload::{message_row, session_row};
use super::protocol::{RpcError, ERR_INVALID_PARAMS};
use super::resources::{parse_aghist_uri, resource_descriptor, session_uri, turn_uri, ParsedUri};
use super::server::McpServer;

use crate::model::{Provider, Session};
use crate::provider::HistoryProvider;

impl McpServer {
    /// Lists every discoverable session as a top-level
    /// `aghist://session/<provider>/<id>` resource. Per-turn URIs are
    /// advertised via the resource template (see `resources/templates/list`)
    /// rather than enumerated, since the turn count would balloon the listing
    /// for large histories.
    pub(super) fn resources_list(&self, _params: &Value) -> Value {
        let mut sessions = self.collect_sessions();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        let resources: Vec<Value> = sessions.iter().map(resource_descriptor).collect();
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
                provider,
                session_id,
            } => self.read_session_resource(provider, &session_id),
            ParsedUri::Turn {
                provider,
                session_id,
                turn,
            } => self.read_turn_resource(provider, &session_id, turn),
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
        provider_want: Provider,
        session_id: &str,
    ) -> Result<Value, String> {
        let (session, provider) = self.find_session_strict(provider_want, session_id)?;
        let messages = provider
            .load_messages(&session)
            .map_err(|e| format!("failed to load messages: {e}"))?;
        let turns: Vec<Value> = messages
            .iter()
            .enumerate()
            .map(|(i, m)| message_row(&session, m, i + 1))
            .collect();
        Ok(json!({
            "uri": session_uri(session.provider, &session.id.0),
            "session": session_row(&session),
            "turns": turns,
        }))
    }

    fn read_turn_resource(
        &self,
        provider_want: Provider,
        session_id: &str,
        turn: u32,
    ) -> Result<Value, String> {
        let (session, provider) = self.find_session_strict(provider_want, session_id)?;
        let messages = provider
            .load_messages(&session)
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
            "uri": turn_uri(session.provider, &session.id.0, turn),
            "session": session_row(&session),
            "turn": message_row(&session, msg, turn_usize),
        }))
    }

    /// Provider-qualified session lookup. Unlike `with_session`, this does NOT
    /// fall through to other providers; a URI names exactly one provider, so
    /// resolving against a different one would silently mask typos.
    fn find_session_strict(
        &self,
        provider_want: Provider,
        session_id: &str,
    ) -> Result<(Session, &dyn HistoryProvider), String> {
        let provider = self
            .providers
            .iter()
            .find(|p| p.provider() == provider_want)
            .ok_or_else(|| {
                format!(
                    "provider '{}' is not enabled or not detected",
                    provider_want.slug()
                )
            })?;
        let sessions = provider
            .discover_sessions()
            .map_err(|e| format!("failed to discover sessions: {e}"))?;
        let session = sessions
            .into_iter()
            .find(|s| s.id.0 == session_id)
            .ok_or_else(|| {
                format!(
                    "session '{}' not found in provider '{}'",
                    session_id,
                    provider_want.slug()
                )
            })?;
        Ok((session, provider.as_ref()))
    }
}
