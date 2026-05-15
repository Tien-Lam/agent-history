use serde_json::{json, Value};

use super::resources::{session_uri, turn_uri};
use crate::model::{Message, Provider, Session};

pub(super) fn tool_definitions() -> Value {
    let provider_slugs = provider_slug_vec();
    json!([
        {
            "name": "search_sessions",
            "description": "Full-text search across indexed sessions. Returns hits with stable citation refs (`<provider>/<session-id>#<turn>`). Refreshes the index incrementally before searching.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Tantivy query string. Matches the `content` and `project` fields." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 20 }
                },
                "required": ["query"]
            }
        },
        {
            "name": "list_sessions",
            "description": "List sessions across enabled providers, sorted by start time descending.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "provider": { "type": "string", "enum": provider_slugs.clone() },
                    "project": { "type": "string", "description": "Substring match on session project_name." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "default": 50 }
                }
            }
        },
        {
            "name": "get_session",
            "description": "Resolve a session by ID (full or unique prefix) and return its metadata plus all turns.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "get_message",
            "description": "Resolve a citation ref `<provider>/<session-id>#<turn>` to the target message, optionally with context turns on each side.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "ref": { "type": "string", "description": "Citation ref. Example: claude-code/abc-123#7" },
                    "include_context": { "type": "integer", "minimum": 0, "maximum": 100, "default": 0 }
                },
                "required": ["ref"]
            }
        },
        {
            "name": "reindex",
            "description": "Refresh the search index (incremental by default). Returns counts of added/updated/unchanged sessions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "provider": { "type": "string", "enum": provider_slugs },
                    "force": { "type": "boolean", "default": false, "description": "Clear the index first for a full rebuild." }
                }
            }
        },
        {
            "name": "health",
            "description": "Run the same checks as `aghist health`: provider detection, index dir writability, manifest sanity, schema presence.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn provider_slug_vec() -> Vec<&'static str> {
    Provider::all().iter().map(|p| p.slug()).collect()
}

pub(super) fn tool_success(payload: &Value) -> Value {
    let text =
        serde_json::to_string_pretty(payload).unwrap_or_else(|_| "<unserializable>".to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": payload,
    })
}

pub(super) fn tool_error(message: impl Into<String>) -> Value {
    let msg = message.into();
    json!({
        "content": [{ "type": "text", "text": msg }],
        "isError": true,
    })
}

pub(super) fn session_row(s: &Session) -> Value {
    json!({
        "id": s.id.0,
        "uri": session_uri(s.provider, &s.id.0),
        "provider": s.provider.slug(),
        "project": s.project_name,
        "branch": s.git_branch,
        "summary": s.summary,
        "model": s.model,
        "started_at": s.started_at,
        "ended_at": s.ended_at,
        "message_count": s.message_count,
    })
}

pub(super) fn message_row(session: &Session, msg: &Message, turn: usize) -> Value {
    let turn_u32 = u32::try_from(turn).unwrap_or(u32::MAX);
    json!({
        "ref": session.citation_ref(turn_u32).map(|r| r.to_string()),
        "uri": turn_uri(session.provider, &session.id.0, turn_u32),
        "turn": turn,
        "id": msg.id.0,
        "role": msg.role,
        "timestamp": msg.timestamp,
        "model": msg.model,
        "content": msg.content,
    })
}
