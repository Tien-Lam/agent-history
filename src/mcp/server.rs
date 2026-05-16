use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use serde_json::{json, Value};

use super::payload::tool_definitions;
use super::protocol::{
    serialize_response, Request, Response, RpcError, ERR_INVALID_REQUEST, ERR_METHOD_NOT_FOUND,
    ERR_PARSE, PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION,
};
use super::resources::resource_templates;

use crate::config::RemoteSource;
use crate::federated::{self, FederatedDiscovery, SourceFailure, LOCAL_SOURCE};
use crate::model::{Provider, Session};
use crate::provider::HistoryProvider;

/// Owns the providers + search index for the lifetime of a server run.
pub struct McpServer {
    pub(super) providers: Vec<Box<dyn HistoryProvider>>,
    sources: Vec<RemoteSource>,
    sources_cache_root: Option<PathBuf>,
    visible_providers: HashSet<Provider>,
}

pub(super) struct LocatedSession {
    pub session: Session,
    pub source: String,
}

impl McpServer {
    pub fn new(providers: Vec<Box<dyn HistoryProvider>>) -> Self {
        let visible_providers = providers.iter().map(|p| p.provider()).collect();
        Self {
            providers,
            sources: Vec::new(),
            sources_cache_root: None,
            visible_providers,
        }
    }

    pub fn new_federated(
        providers: Vec<Box<dyn HistoryProvider>>,
        sources: Vec<RemoteSource>,
        sources_cache_root: Option<PathBuf>,
        visible_providers: HashSet<Provider>,
    ) -> Self {
        Self {
            providers,
            sources,
            sources_cache_root,
            visible_providers,
        }
    }

    /// Drives the loop reading newline-delimited JSON from `input` and writing
    /// responses to `output`. Returns when stdin reaches EOF.
    pub fn serve<R: BufRead, W: Write>(&self, input: R, mut output: W) -> io::Result<()> {
        for line in input.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let response = self.handle_line(trimmed);
            if let Some(json_line) = response {
                output.write_all(json_line.as_bytes())?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
        Ok(())
    }

    /// Returns the JSON line to send back, or `None` for notifications.
    fn handle_line(&self, line: &str) -> Option<String> {
        let request: Request = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                return Some(serialize_response(&Response {
                    jsonrpc: "2.0",
                    id: Value::Null,
                    result: None,
                    error: Some(RpcError::new(ERR_PARSE, format!("parse error: {e}"))),
                }));
            }
        };

        let is_notification = request.id.is_none();
        let id = request.id.clone().unwrap_or(Value::Null);

        if !request.jsonrpc.is_empty() && request.jsonrpc != "2.0" {
            if is_notification {
                return None;
            }
            return Some(serialize_response(&Response {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(RpcError::new(
                    ERR_INVALID_REQUEST,
                    format!(
                        "unsupported jsonrpc version '{}' (expected '2.0')",
                        request.jsonrpc
                    ),
                )),
            }));
        }

        let outcome = self.dispatch(&request.method, &request.params);

        if is_notification {
            return None;
        }

        let resp = match outcome {
            Ok(value) => Response {
                jsonrpc: "2.0",
                id,
                result: Some(value),
                error: None,
            },
            Err(err) => Response {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(err),
            },
        };
        Some(serialize_response(&resp))
    }

    fn dispatch(&self, method: &str, params: &Value) -> Result<Value, RpcError> {
        match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false }
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION,
                }
            })),
            // The client sends `notifications/initialized` after `initialize`;
            // it has no id and we just ignore it. `shutdown` is a no-op for
            // stdio (the client closes stdin to terminate).
            "notifications/initialized" | "initialized" | "shutdown" => Ok(Value::Null),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => self.tools_call(params),
            "resources/list" => Ok(self.resources_list(params)),
            "resources/read" => self.resources_read(params),
            "resources/templates/list" => Ok(json!({ "resourceTemplates": resource_templates() })),
            other => Err(RpcError::new(
                ERR_METHOD_NOT_FOUND,
                format!("method not found: {other}"),
            )),
        }
    }

    // --- helpers -----------------------------------------------------------

    pub(super) fn collect_discovery(&self) -> FederatedDiscovery {
        let mut discovery = if let Some(cache_root) = self.sources_cache_root.as_ref() {
            federated::discover_federated(&self.providers, &self.sources, cache_root)
        } else {
            let mut local = self.collect_local_discovery();
            if !self.sources.is_empty() {
                local.failures.push(SourceFailure {
                    source: LOCAL_SOURCE.to_string(),
                    message: "sources cache dir unavailable".to_string(),
                });
            }
            local
        };
        discovery.retain_providers(&self.visible_providers);
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

    pub(super) fn provider_scope(&self) -> HashSet<Provider> {
        self.visible_providers.clone()
    }

    pub(super) fn find_session_by_prefix(
        &self,
        session_id: &str,
        provider_filter: Option<Provider>,
        source_filter: Option<&str>,
    ) -> Result<LocatedSession, String> {
        if let Some(provider) = provider_filter {
            self.ensure_provider_visible(provider)?;
        }
        let discovery = self.collect_discovery();
        let candidates: Vec<LocatedSession> = discovery
            .sessions
            .iter()
            .filter(|session| provider_filter.is_none_or(|want| session.provider == want))
            .filter(|session| {
                source_filter.is_none_or(|want| discovery.source_of_session(session) == want)
            })
            .filter(|session| session.id.0 == session_id || session.id.0.starts_with(session_id))
            .map(|session| LocatedSession {
                session: session.clone(),
                source: discovery.source_of_session(session).to_string(),
            })
            .collect();

        let exact: Vec<LocatedSession> = candidates
            .iter()
            .filter(|located| located.session.id.0 == session_id)
            .map(|located| LocatedSession {
                session: located.session.clone(),
                source: located.source.clone(),
            })
            .collect();
        let matches = if exact.is_empty() { candidates } else { exact };

        match matches.len() {
            0 => Err(format!("session not found: {session_id}")),
            1 => Ok(matches.into_iter().next().expect("len checked")),
            _ => Err(format!(
                "session id '{session_id}' is ambiguous; specify provider and source"
            )),
        }
    }

    pub(super) fn find_session_exact(
        &self,
        provider: Provider,
        session_id: &str,
        source: &str,
    ) -> Result<LocatedSession, String> {
        self.ensure_provider_visible(provider)?;
        let discovery = self.collect_discovery();
        discovery
            .sessions
            .iter()
            .find(|session| {
                session.provider == provider
                    && session.id.0 == session_id
                    && discovery.source_of_session(session) == source
            })
            .map(|session| LocatedSession {
                session: session.clone(),
                source: discovery.source_of_session(session).to_string(),
            })
            .ok_or_else(|| {
                format!(
                    "session '{session_id}' not found in provider '{}' from source '{source}'",
                    provider.slug()
                )
            })
    }

    fn ensure_provider_visible(&self, provider: Provider) -> Result<(), String> {
        if self.visible_providers.contains(&provider) {
            Ok(())
        } else {
            Err(format!(
                "provider '{}' is not enabled or not visible to MCP",
                provider.slug()
            ))
        }
    }
}
