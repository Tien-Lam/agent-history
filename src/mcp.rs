//! Stdio MCP server: exposes aghist's read paths over JSON-RPC 2.0.
//!
//! Transport is newline-delimited JSON on stdin/stdout per the MCP stdio spec.
//! Logs go through `tracing` to a file (configured in `main`); nothing else
//! may touch stdout while the server is running.
//!
//! Tools exposed (see `tools/list`):
//! - `search_sessions` — full-text search, returns hits keyed by citation ref
//! - `list_sessions`   — provider-aware session listing
//! - `get_session`     — one session's metadata + ordered turns
//! - `get_message`     — resolves a citation ref `<provider>/<id>#<turn>`
//! - `reindex`         — incremental or `--force` rebuild of the search index
//! - `health`          — same checks as `aghist health`
//!
//! Resources exposed (see `resources/list` / `resources/read`):
//! - `aghist://session/<provider>/<session-id>` — session metadata + all turns
//! - `aghist://session/<provider>/<session-id>/turn/<n>` — single turn (1-based)
//!
//! The URI shape mirrors the citation-ref triple so URIs are stable across
//! reindex: provider slug + session id are intrinsic to the source data, and
//! turn `n` is the load-order position of the message within the session.
//!
//! ## Read-only contract
//!
//! No tool or resource exposed by this server may mutate provider history. The
//! `HistoryProvider` trait deliberately offers only read methods
//! (`discover_sessions`, `load_messages`) — there is no write surface to call.
//! Tool calls may rebuild the local Tantivy index (a derived cache under
//! `~/.aghist/`), but they never write back to the upstream session files.
//! Adding a tool that violates this contract requires loosening the trait,
//! which should be a deliberate design change — not a quiet edit here.
//!
//! ## Provider scoping
//!
//! The server only sees the providers handed to `McpServer::new`. `main` filters
//! `config.enabled_providers()` further by `config.mcp_exposed_providers()` so
//! users can hide a provider from MCP clients without disabling it for the TUI.
//! When the resulting list is empty, every tool that walks providers returns
//! an empty result rather than erroring — the same behaviour as having no
//! sessions discovered.

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

mod args;
mod payload;
mod protocol;
mod resources;

use args::{optional_provider, optional_str, optional_usize, required_str};
use payload::{message_row, session_row, tool_definitions, tool_error, tool_success};
pub use protocol::PROTOCOL_VERSION;
use protocol::{
    serialize_response, Request, Response, RpcError, ERR_INVALID_PARAMS, ERR_INVALID_REQUEST,
    ERR_METHOD_NOT_FOUND, ERR_PARSE, SERVER_NAME, SERVER_VERSION,
};
use resources::{
    parse_aghist_uri, resource_descriptor, resource_templates, session_uri, turn_uri, ParsedUri,
};

use crate::health::run_health_checks;
use crate::model::{CitationRef, Provider, Session};
use crate::provider::HistoryProvider;
use crate::search::SearchIndex;

/// Owns the providers + search index for the lifetime of a server run.
pub struct McpServer {
    providers: Vec<Box<dyn HistoryProvider>>,
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

    fn tools_call(&self, params: &Value) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(ERR_INVALID_PARAMS, "missing 'name' field"))?;
        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

        // Tool-level errors are reported in MCP's content envelope (isError=true),
        // not as JSON-RPC errors — so the model can read the message.
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

    // ─── individual tools ──────────────────────────────────────────────────

    fn tool_search_sessions(&self, args: &Value) -> Result<Value, String> {
        let query = required_str(args, "query")?;
        if query.trim().is_empty() {
            return Err("query is empty".to_string());
        }
        let limit = optional_usize(args, "limit", 20, 1, 200)?;

        let sessions = self.collect_sessions();
        let index_dir = SearchIndex::default_index_dir();
        let index = SearchIndex::open_or_create(&index_dir)
            .map_err(|e| format!("failed to open search index: {e}"))?;

        let (tx, _rx) = crossbeam_channel::unbounded::<crate::action::Action>();
        index
            .build_index(&sessions, &self.providers, &tx)
            .map_err(|e| format!("failed to build index: {e}"))?;

        // Best-effort: surface user notes alongside session messages. Sidecar
        // failures are intentionally swallowed — a missing metadata.db is the
        // common case and must not break MCP search.
        if let Some(path) = crate::metadata::default_path() {
            if path.exists() {
                if let Ok(conn) = crate::metadata::open(&path) {
                    if let Ok(notes) = crate::metadata::note_list(&conn, None) {
                        let _ = index.index_notes(&notes);
                    }
                }
            }
        }

        let hits = index
            .search(&query, limit)
            .map_err(|e| format!("search failed: {e}"))?;

        let session_meta: std::collections::HashMap<String, &Session> =
            sessions.iter().map(|s| (s.identity_key(), s)).collect();

        // Resolve message_id -> 1-based turn by loading messages once per
        // unique session that appears in the hit set. Without this the caller
        // can't construct a citation ref from a search hit. Sessions that
        // can't be loaded are silently dropped from the turn map; their hits
        // get `ref: null` and `turn: null`.
        let mut turn_lookup: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut seen_sessions: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for h in &hits {
            if !seen_sessions.insert(h.session_key.as_str()) {
                continue;
            }
            let Some(session) = session_meta.get(h.session_key.as_str()).copied() else {
                continue;
            };
            let Some(provider) = self
                .providers
                .iter()
                .find(|p| p.provider() == session.provider)
            else {
                continue;
            };
            let Ok(messages) = provider.load_messages(session) else {
                continue;
            };
            for (i, m) in messages.iter().enumerate() {
                turn_lookup.insert(session.message_key(i, &m.id.0), i + 1);
            }
        }

        let mut hits_json = Vec::with_capacity(hits.len());
        for h in &hits {
            match h.kind {
                crate::search::HitKind::Note => {
                    // Notes carry their own ref shape (`<provider>/<id>[#<turn>]`)
                    // so we surface it directly. Per-message fields don't apply.
                    hits_json.push(json!({
                        "kind": crate::search::HitKind::Note.slug(),
                        "ref": h.note_session_ref,
                        "note_id": h.note_id,
                        "score": h.score,
                        "snippet": h.snippet,
                    }));
                }
                crate::search::HitKind::Message => {
                    let session = session_meta.get(h.session_key.as_str()).copied();
                    let turn = turn_lookup.get(h.message_key.as_str()).copied();
                    let citation_ref = session
                        .zip(turn)
                        .and_then(|(s, t)| {
                            u32::try_from(t).ok().and_then(|turn| s.citation_ref(turn))
                        })
                        .map(|r| r.to_string());
                    hits_json.push(json!({
                        "kind": crate::search::HitKind::Message.slug(),
                        "ref": citation_ref,
                        "session_id": h.session_id,
                        "message_id": h.message_id,
                        "turn": turn,
                        "score": h.score,
                        "snippet": h.snippet,
                        "provider": session.map(|s| s.provider.slug()),
                        "project": session.and_then(|s| s.project_name.as_deref()),
                        "started_at": session.map(|s| s.started_at),
                    }));
                }
            }
        }

        Ok(json!({
            "query": query,
            "limit": limit,
            "total": hits_json.len(),
            "hits": hits_json,
        }))
    }

    fn tool_list_sessions(&self, args: &Value) -> Result<Value, String> {
        let provider_filter = optional_provider(args, "provider")?;
        let project_filter = optional_str(args, "project")?;
        let limit = optional_usize(args, "limit", 50, 1, 1_000)?;

        let mut all = self.collect_sessions();
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

        let rows: Vec<Value> = filtered.iter().map(|s| session_row(s)).collect();

        Ok(json!({
            "total": rows.len(),
            "sessions": rows,
        }))
    }

    fn tool_get_session(&self, args: &Value) -> Result<Value, String> {
        let session_id = required_str(args, "session_id")?;
        self.with_session(&session_id, |session, provider| {
            let messages = provider
                .load_messages(session)
                .map_err(|e| format!("failed to load messages for {}: {e}", session.id.0))?;
            let turns: Vec<Value> = messages
                .iter()
                .enumerate()
                .map(|(i, m)| message_row(session, m, i + 1))
                .collect();
            Ok(json!({
                "session": session_row(session),
                "turns": turns,
            }))
        })
    }

    fn tool_get_message(&self, args: &Value) -> Result<Value, String> {
        let raw_ref = required_str(args, "ref")?;
        let citation: CitationRef = raw_ref
            .parse()
            .map_err(|e| format!("invalid ref '{raw_ref}': {e}"))?;
        let include_context = optional_usize(args, "include_context", 0, 0, 100)?;

        let provider = self
            .providers
            .iter()
            .find(|p| p.provider() == citation.provider)
            .ok_or_else(|| {
                format!(
                    "provider '{}' is not enabled or not detected",
                    citation.provider.slug()
                )
            })?;

        let sessions = provider
            .discover_sessions()
            .map_err(|e| format!("failed to discover sessions: {e}"))?;
        let session = sessions
            .iter()
            .find(|s| s.id == citation.session_id)
            .ok_or_else(|| {
                format!(
                    "session '{}' not found in provider '{}'",
                    citation.session_id,
                    citation.provider.slug()
                )
            })?;
        let messages = provider
            .load_messages(session)
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
                let mut row = message_row(session, m, start_idx + i + 1);
                if let Some(obj) = row.as_object_mut() {
                    obj.insert("is_target".to_string(), json!(start_idx + i == target_idx));
                }
                row
            })
            .collect();

        Ok(json!({
            "ref": citation.to_string(),
            "session": session_row(session),
            "target_turn": citation.turn,
            "turns": turns,
        }))
    }

    fn tool_reindex(&self, args: &Value) -> Result<Value, String> {
        let provider_filter = optional_provider(args, "provider")?;
        let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);

        let active: Vec<&Box<dyn HistoryProvider>> = self
            .providers
            .iter()
            .filter(|p| provider_filter.is_none_or(|want| p.provider() == want))
            .collect();

        if let Some(want) = provider_filter {
            if active.is_empty() {
                return Err(format!(
                    "provider '{}' is not enabled or not detected",
                    want.slug()
                ));
            }
        }

        let mut sessions: Vec<Session> = Vec::new();
        let mut errors: Vec<Value> = Vec::new();
        for p in &active {
            match p.discover_sessions() {
                Ok(s) => sessions.extend(s),
                Err(e) => errors.push(json!({
                    "provider": p.provider().slug(),
                    "error": e.to_string(),
                })),
            }
        }

        let index_dir = SearchIndex::default_index_dir();
        let index = SearchIndex::open_or_create(&index_dir)
            .map_err(|e| format!("failed to open index at {}: {e}", index_dir.display()))?;
        if force {
            index
                .clear()
                .map_err(|e| format!("failed to clear index: {e}"))?;
        }

        let started = std::time::Instant::now();
        let (tx, _rx) = crossbeam_channel::unbounded::<crate::action::Action>();
        let stats = index
            .build_index(&sessions, &self.providers, &tx)
            .map_err(|e| format!("failed to build index: {e}"))?;

        let provider_slugs: Vec<&str> = active.iter().map(|p| p.provider().slug()).collect();

        Ok(json!({
            "providers": provider_slugs,
            "sessions_total": sessions.len(),
            "added": stats.added,
            "updated": stats.updated,
            "unchanged": stats.unchanged,
            "messages_indexed": stats.messages_indexed,
            "force": force,
            "index_dir": index_dir.display().to_string(),
            "duration_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "errors": errors,
        }))
    }

    fn tool_health(&self, _args: &Value) -> Value {
        let checks = run_health_checks(&self.providers);
        let any_failed = checks
            .iter()
            .any(|c| c.status == crate::health::HealthStatus::Fail);
        let summary = json!({
            "ok_count": checks.iter().filter(|c| c.status == crate::health::HealthStatus::Ok).count(),
            "warn_count": checks.iter().filter(|c| c.status == crate::health::HealthStatus::Warn).count(),
            "fail_count": checks.iter().filter(|c| c.status == crate::health::HealthStatus::Fail).count(),
        });
        json!({
            "ok": !any_failed,
            "checks": checks,
            "summary": summary,
        })
    }

    // ─── resources ─────────────────────────────────────────────────────────

    /// Lists every discoverable session as a top-level `aghist://session/<provider>/<id>`
    /// resource. Per-turn URIs are advertised via the resource template (see
    /// `resources/templates/list`) rather than enumerated, since the turn count
    /// would balloon the listing for large histories.
    fn resources_list(&self, _params: &Value) -> Value {
        let mut sessions = self.collect_sessions();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        let resources: Vec<Value> = sessions.iter().map(resource_descriptor).collect();
        json!({ "resources": resources })
    }

    fn resources_read(&self, params: &Value) -> Result<Value, RpcError> {
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
    /// fall through to other providers — a URI names exactly one provider, so
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

    // ─── helpers ───────────────────────────────────────────────────────────

    fn collect_sessions(&self) -> Vec<Session> {
        let mut all = Vec::new();
        for p in &self.providers {
            if let Ok(found) = p.discover_sessions() {
                all.extend(found);
            }
        }
        all
    }

    /// Walks providers, discovers sessions, and runs `f` against the matching
    /// session and provider. Used by tools that need to operate on a single
    /// session — keeps `Vec<Session>` alive only for the closure body so we
    /// don't have to thread lifetimes through `Box<dyn HistoryProvider>`.
    fn with_session<T>(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn server() -> McpServer {
        McpServer::new(Vec::new())
    }

    fn run_one(server: &McpServer, request: &str) -> Value {
        let input = format!("{request}\n");
        let mut output = Vec::new();
        server
            .serve(Cursor::new(input.as_bytes()), &mut output)
            .unwrap();
        let line = String::from_utf8(output).unwrap();
        serde_json::from_str(line.trim()).unwrap()
    }

    #[test]
    fn initialize_returns_protocol_and_server_info() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        );
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(resp["result"]["serverInfo"]["name"], "aghist");
        assert!(resp["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn notification_produces_no_response() {
        let mut output = Vec::new();
        server()
            .serve(
                Cursor::new(
                    b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n".as_slice(),
                ),
                &mut output,
            )
            .unwrap();
        assert!(output.is_empty(), "got unexpected response: {output:?}");
    }

    #[test]
    fn tools_list_advertises_all_tools() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        );
        let names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        for expected in [
            "search_sessions",
            "list_sessions",
            "get_session",
            "get_message",
            "reindex",
            "health",
        ] {
            assert!(
                names.contains(&expected),
                "missing tool {expected} in {names:?}"
            );
        }
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":3,"method":"nope/nope"}"#,
        );
        assert_eq!(resp["error"]["code"], ERR_METHOD_NOT_FOUND);
    }

    #[test]
    fn malformed_json_returns_parse_error() {
        let resp = run_one(&server(), "not json");
        assert_eq!(resp["error"]["code"], ERR_PARSE);
    }

    #[test]
    fn unknown_tool_returns_tool_error_envelope() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"bogus","arguments":{}}}"#,
        );
        assert_eq!(resp["result"]["isError"], true);
        let txt = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(txt.contains("unknown tool"), "got: {txt}");
    }

    #[test]
    fn list_sessions_with_no_providers_returns_empty() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_sessions","arguments":{}}}"#,
        );
        assert_eq!(resp["result"]["isError"], false);
        let structured = &resp["result"]["structuredContent"];
        assert_eq!(structured["total"], 0);
        assert!(structured["sessions"].as_array().unwrap().is_empty());
    }

    #[test]
    fn get_message_with_invalid_ref_reports_error() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"get_message","arguments":{"ref":"not-a-ref"}}}"#,
        );
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn get_message_with_missing_ref_arg_reports_error() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"get_message","arguments":{}}}"#,
        );
        assert_eq!(resp["result"]["isError"], true);
        let txt = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(txt.contains("ref"), "got: {txt}");
    }

    #[test]
    fn list_sessions_validates_provider_slug() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"list_sessions","arguments":{"provider":"made-up"}}}"#,
        );
        assert_eq!(resp["result"]["isError"], true);
        let txt = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(txt.contains("made-up"), "got: {txt}");
    }

    #[test]
    fn list_sessions_rejects_out_of_range_limit() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"list_sessions","arguments":{"limit":0}}}"#,
        );
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn initialize_advertises_resources_capability() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        );
        let caps = &resp["result"]["capabilities"];
        assert!(
            caps["resources"].is_object(),
            "missing resources cap: {caps}"
        );
        assert_eq!(caps["resources"]["subscribe"], false);
        assert_eq!(caps["resources"]["listChanged"], false);
    }

    #[test]
    fn resources_list_with_no_providers_returns_empty() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#,
        );
        assert_eq!(resp["error"], Value::Null);
        assert!(resp["result"]["resources"].as_array().unwrap().is_empty());
    }

    #[test]
    fn resources_templates_list_advertises_session_and_turn() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/templates/list"}"#,
        );
        let templates = resp["result"]["resourceTemplates"].as_array().unwrap();
        let uris: Vec<&str> = templates
            .iter()
            .map(|t| t["uriTemplate"].as_str().unwrap())
            .collect();
        assert!(uris.contains(&"aghist://session/{provider}/{session_id}"));
        assert!(uris.contains(&"aghist://session/{provider}/{session_id}/turn/{turn}"));
    }

    #[test]
    fn resources_read_missing_uri_is_invalid_params() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{}}"#,
        );
        assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
    }

    #[test]
    fn resources_read_rejects_non_aghist_scheme() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"file:///etc/passwd"}}"#,
        );
        assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
    }

    #[test]
    fn resources_read_rejects_unknown_provider() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"aghist://session/made-up/abc"}}"#,
        );
        assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
        let msg = resp["error"]["message"].as_str().unwrap();
        assert!(msg.contains("made-up"), "got: {msg}");
    }

    #[test]
    fn resources_read_rejects_unknown_session_for_known_provider() {
        // No providers wired up, so the lookup fails on provider-not-enabled
        // before it can reach session resolution. Either way: invalid params.
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"aghist://session/claude-code/abc"}}"#,
        );
        assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
    }

    #[test]
    fn resources_read_rejects_zero_turn() {
        let resp = run_one(
            &server(),
            r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"aghist://session/claude-code/abc/turn/0"}}"#,
        );
        assert_eq!(resp["error"]["code"], ERR_INVALID_PARAMS);
        let msg = resp["error"]["message"].as_str().unwrap();
        assert!(msg.contains("turn"), "got: {msg}");
    }

    #[test]
    fn parse_uri_session_form() {
        let p = parse_aghist_uri("aghist://session/claude-code/abc-123").unwrap();
        match p {
            ParsedUri::Session {
                provider,
                session_id,
            } => {
                assert_eq!(provider, Provider::ClaudeCode);
                assert_eq!(session_id, "abc-123");
            }
            ParsedUri::Turn { .. } => panic!("expected Session form"),
        }
    }

    #[test]
    fn parse_uri_turn_form() {
        let p = parse_aghist_uri("aghist://session/codex-cli/ses_abc/turn/7").unwrap();
        match p {
            ParsedUri::Turn {
                provider,
                session_id,
                turn,
            } => {
                assert_eq!(provider, Provider::CodexCli);
                assert_eq!(session_id, "ses_abc");
                assert_eq!(turn, 7);
            }
            ParsedUri::Session { .. } => panic!("expected Turn form"),
        }
    }

    #[test]
    fn parse_uri_session_id_with_slash_in_path_is_treated_as_session_id() {
        // No real provider emits these today, but if a session id ever contains
        // a `/`, anything before `/turn/<n>` should still parse as the id.
        let p = parse_aghist_uri("aghist://session/claude-code/foo/bar/turn/3").unwrap();
        match p {
            ParsedUri::Turn {
                session_id, turn, ..
            } => {
                assert_eq!(session_id, "foo/bar");
                assert_eq!(turn, 3);
            }
            ParsedUri::Session { .. } => panic!("expected Turn form"),
        }
    }

    #[test]
    fn parse_uri_rejects_bad_inputs() {
        assert!(parse_aghist_uri("file:///etc/passwd").is_err());
        assert!(parse_aghist_uri("aghist://session/").is_err());
        assert!(parse_aghist_uri("aghist://session/claude-code").is_err());
        assert!(parse_aghist_uri("aghist://session/claude-code/").is_err());
        assert!(parse_aghist_uri("aghist://session/claude-code/abc/turn/").is_err());
        assert!(parse_aghist_uri("aghist://session/claude-code/abc/turn/abc").is_err());
        assert!(parse_aghist_uri("aghist://session/claude-code/abc/turn/0").is_err());
    }

    #[test]
    fn session_uri_round_trips_through_parser() {
        let uri = session_uri(Provider::OpenCode, "session-xyz");
        assert_eq!(uri, "aghist://session/opencode/session-xyz");
        let parsed = parse_aghist_uri(&uri).unwrap();
        match parsed {
            ParsedUri::Session {
                provider,
                session_id,
            } => {
                assert_eq!(provider, Provider::OpenCode);
                assert_eq!(session_id, "session-xyz");
            }
            ParsedUri::Turn { .. } => panic!("expected Session"),
        }
    }

    #[test]
    fn turn_uri_round_trips_through_parser() {
        let uri = turn_uri(Provider::GeminiCli, "g-1", 42);
        assert_eq!(uri, "aghist://session/gemini-cli/g-1/turn/42");
        let parsed = parse_aghist_uri(&uri).unwrap();
        match parsed {
            ParsedUri::Turn {
                provider,
                session_id,
                turn,
            } => {
                assert_eq!(provider, Provider::GeminiCli);
                assert_eq!(session_id, "g-1");
                assert_eq!(turn, 42);
            }
            ParsedUri::Session { .. } => panic!("expected Turn"),
        }
    }

    #[test]
    fn ping_returns_empty_object() {
        let resp = run_one(&server(), r#"{"jsonrpc":"2.0","id":10,"method":"ping"}"#);
        assert!(resp["result"].is_object());
        assert!(resp["error"].is_null());
    }
}
