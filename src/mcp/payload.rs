use serde_json::{json, Value};

use crate::dto::{McpSessionRow, MessageRow};
use crate::federated::LOCAL_SOURCE;
use crate::model::{Message, QualifiedCitationRef, Session};
use crate::schema_fragments;

use super::resources::{session_uri_for_source, turn_uri_for_source};

pub(super) fn tool_definitions() -> Value {
    let provider_slugs = schema_fragments::provider_slug_enum();
    let citation_ref_pattern = schema_fragments::source_qualified_citation_ref_pattern();
    let search_limit_default = schema_fragments::SEARCH_LIMIT_DEFAULT;
    let search_limit_max = schema_fragments::MCP_SEARCH_LIMIT_MAX;
    let list_limit_default = schema_fragments::MCP_LIST_LIMIT_DEFAULT;
    let list_limit_max = schema_fragments::MCP_LIST_LIMIT_MAX;
    let include_context_default = schema_fragments::MCP_INCLUDE_CONTEXT_DEFAULT;
    let include_context_max = schema_fragments::MCP_INCLUDE_CONTEXT_MAX;
    let mcp_get_message_response_schema = schema_fragments::mcp_get_message_response_schema();
    let mcp_get_session_response_schema = schema_fragments::mcp_get_session_response_schema();
    let mcp_list_response_schema = schema_fragments::mcp_list_response_schema();
    let mcp_search_response_schema = schema_fragments::mcp_search_response_schema();
    json!([
        {
            "name": "search_sessions",
            "description": "Full-text search across indexed sessions. Returns hits with stable citation refs (`<provider>/<session-id>#<turn>` locally, `<source>:<provider>/<session-id>#<turn>` for remote sources). Refreshes the index incrementally before searching.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Tantivy query string. Matches the `content` and `project` fields." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": search_limit_max, "default": search_limit_default }
                },
                "required": ["query"],
                "additionalProperties": false
            },
            "outputSchema": mcp_search_response_schema
        },
        {
            "name": "list_sessions",
            "description": "List sessions across MCP-visible local providers and registered remote source caches, sorted by start time descending.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "provider": { "type": "string", "enum": provider_slugs.clone() },
                    "project": { "type": "string", "description": "Substring match on session project_name." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": list_limit_max, "default": list_limit_default }
                },
                "additionalProperties": false
            },
            "outputSchema": mcp_list_response_schema
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
            },
            "outputSchema": mcp_get_session_response_schema
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
                    "include_context": { "type": "integer", "minimum": 0, "maximum": include_context_max, "default": include_context_default }
                },
                "required": ["ref"],
                "additionalProperties": false
            },
            "outputSchema": mcp_get_message_response_schema
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
    let row = McpSessionRow::from_session(
        s,
        source,
        session_uri_for_source(source, s.provider, &s.id.0),
    );
    serde_json::to_value(row).unwrap_or(Value::Null)
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
    let row = MessageRow::from_message(
        msg,
        source,
        turn,
        ref_,
        turn_uri_for_source(source, session.provider, &session.id.0, turn_u32),
    );
    serde_json::to_value(row).unwrap_or(Value::Null)
}
