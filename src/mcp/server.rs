use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use super::args::{optional_provider, optional_str, optional_usize, required_str};
use super::payload::{message_row, session_row, tool_definitions, tool_error, tool_success};
use super::protocol::{
    serialize_response, Request, Response, RpcError, ERR_INVALID_PARAMS, ERR_INVALID_REQUEST,
    ERR_METHOD_NOT_FOUND, ERR_PARSE, PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION,
};
use super::resources::{
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
