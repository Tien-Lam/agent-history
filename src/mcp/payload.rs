use serde_json::{json, Value};

use super::resources::{session_uri_for_source, turn_uri_for_source};
use crate::federated::LOCAL_SOURCE;
use crate::model::{Message, QualifiedCitationRef, Session};
use crate::schema_fragments;

pub(super) fn tool_definitions() -> Value {
    let provider_slugs = schema_fragments::provider_slug_enum();
    let citation_ref_pattern = schema_fragments::source_qualified_citation_ref_pattern();
    json!([
        {
            "name": "search_sessions",
            "description": "Full-text search across indexed sessions. Returns hits with stable citation refs (`<provider>/<session-id>#<turn>` locally, `<source>:<provider>/<session-id>#<turn>` for remote sources). Refreshes the index incrementally before searching.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Tantivy query string. Matches the `content` and `project` fields." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 20 }
                },
                "required": ["query"],
                "additionalProperties": false
            }
        },
        {
            "name": "list_sessions",
            "description": "List sessions across MCP-visible local providers and registered remote source caches, sorted by start time descending.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "provider": { "type": "string", "enum": provider_slugs.clone() },
                    "project": { "type": "string", "description": "Substring match on session project_name." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "default": 50 }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "get_session",
            "description": "Resolve a session by ID (full or unique prefix) and return its metadata plus all turns. Use provider/source to disambiguate federated sessions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "provider": { "type": "string", "enum": provider_slugs.clone() },
                    "source": { "type": "string", "description": "Source name from list_sessions. Omit for unique matches; use 'local' for local-only lookup." }
                },
                "required": ["session_id"],
                "additionalProperties": false
            }
        },
        {
            "name": "get_message",
            "description": "Resolve a citation ref `<provider>/<session-id>#<turn>` or `<source>:<provider>/<session-id>#<turn>` to the target message, optionally with context turns on each side.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "pattern": citation_ref_pattern,
                        "description": "Citation ref. Example: claude-code/abc-123#7"
                    },
                    "include_context": { "type": "integer", "minimum": 0, "maximum": 100, "default": 0 }
                },
                "required": ["ref"],
                "additionalProperties": false
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
                },
                "additionalProperties": false
            }
        },
        {
            "name": "health",
            "description": "Run the same checks as `aghist health`: provider detection, index dir writability, manifest sanity, schema presence.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }
    ])
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

pub(super) fn session_row_with_source(s: &Session, source: &str) -> Value {
    json!({
        "id": s.id.0,
        "uri": session_uri_for_source(source, s.provider, &s.id.0),
        "provider": s.provider.slug(),
        "source": source,
        "project": s.project_name,
        "branch": s.git_branch,
        "summary": s.summary,
        "model": s.model,
        "started_at": s.started_at,
        "ended_at": s.ended_at,
        "message_count": s.message_count,
    })
}

pub(super) fn message_row_with_source(
    session: &Session,
    msg: &Message,
    turn: usize,
    source: &str,
) -> Value {
    let turn_u32 = u32::try_from(turn).unwrap_or(u32::MAX);
    let ref_ = session.citation_ref(turn_u32).map(|citation| {
        QualifiedCitationRef::new(
            (source != LOCAL_SOURCE).then(|| source.to_string()),
            citation,
        )
        .to_string()
    });
    json!({
        "ref": ref_,
        "uri": turn_uri_for_source(source, session.provider, &session.id.0, turn_u32),
        "source": source,
        "turn": turn,
        "id": msg.id.0,
        "role": msg.role,
        "timestamp": msg.timestamp,
        "model": msg.model,
        "content": msg.content,
    })
}
