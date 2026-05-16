use std::collections::HashSet;
use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use super::payload::tool_definitions;
use super::protocol::{
    serialize_response, Request, Response, RpcError, ERR_INVALID_REQUEST, ERR_METHOD_NOT_FOUND,
    ERR_PARSE, PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION,
};
use super::resources::resource_templates;

use crate::model::{Provider, Session};
use crate::provider::HistoryProvider;

/// Owns the providers + search index for the lifetime of a server run.
pub struct McpServer {
    pub(super) providers: Vec<Box<dyn HistoryProvider>>,
}

impl McpServer {
    pub fn new(providers: Vec<Box<dyn HistoryProvider>>) -> Self {
        Self { providers }
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

    pub(super) fn collect_sessions(&self) -> Vec<Session> {
        let mut all = Vec::new();
        for p in &self.providers {
            if let Ok(found) = p.discover_sessions() {
                all.extend(found);
            }
        }
        all
    }

    pub(super) fn provider_scope(&self) -> HashSet<Provider> {
        self.providers.iter().map(|p| p.provider()).collect()
    }

    /// Walks providers, discovers sessions, and runs `f` against the matching
    /// session and provider. Used by tools that need to operate on a single
    /// session; keeps `Vec<Session>` alive only for the closure body so we
    /// don't have to thread lifetimes through `Box<dyn HistoryProvider>`.
    pub(super) fn with_session<T>(
        &self,
        session_id: &str,
        f: impl FnOnce(&Session, &dyn HistoryProvider) -> Result<T, String>,
    ) -> Result<T, String> {
        for p in &self.providers {
            let Ok(sessions) = p.discover_sessions() else {
                continue;
            };
            if let Some(found) = sessions
                .iter()
                .find(|s| s.id.0 == session_id || s.id.0.starts_with(session_id))
            {
                return f(found, p.as_ref());
            }
        }
        Err(format!("session not found: {session_id}"))
    }
}
