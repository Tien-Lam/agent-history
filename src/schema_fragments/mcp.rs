use serde_json::{json, Value};

use crate::config::MAX_SOURCE_NAME_BYTES;

use super::common::{
    closed_empty_object_schema, closed_object_schema, provider_slug_enum, schema_props,
    source_qualified_citation_ref_pattern,
};
use super::responses::{
    health_response_schema, mcp_get_message_response_schema, mcp_get_session_response_schema,
    mcp_list_response_schema, mcp_reindex_response_schema, mcp_search_response_schema,
};
use super::{
    MCP_FILTER_STRING_MAX_BYTES, MCP_INCLUDE_CONTEXT_DEFAULT, MCP_INCLUDE_CONTEXT_MAX,
    MCP_LIST_LIMIT_DEFAULT, MCP_LIST_LIMIT_MAX, MCP_LOOKUP_STRING_MAX_BYTES, MCP_SEARCH_LIMIT_MAX,
    SEARCH_LIMIT_DEFAULT, SEARCH_QUERY_MAX_BYTES,
};

pub(crate) struct McpToolContract {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) input_schema: Value,
    pub(crate) output_schema: Option<Value>,
}

pub(crate) fn mcp_tool_contracts() -> Vec<McpToolContract> {
    vec![
        McpToolContract {
            name: "search_sessions",
            description: "Full-text search across indexed sessions. Returns hits with stable citation refs (`<provider>/<session-id>#<turn>` locally, `<source>:<provider>/<session-id>#<turn>` for remote sources). Refreshes the index incrementally before searching.",
            input_schema: mcp_search_sessions_input_schema(),
            output_schema: Some(mcp_search_response_schema()),
        },
        McpToolContract {
            name: "list_sessions",
            description: "List sessions across MCP-visible local providers and registered remote source caches, sorted by start time descending.",
            input_schema: mcp_list_sessions_input_schema(),
            output_schema: Some(mcp_list_response_schema()),
        },
        McpToolContract {
            name: "get_session",
            description: "Resolve a session by ID (full or unique prefix) and return its metadata plus all turns. Use provider/source to disambiguate federated sessions.",
            input_schema: mcp_get_session_input_schema(),
            output_schema: Some(mcp_get_session_response_schema()),
        },
        McpToolContract {
            name: "get_message",
            description: "Resolve a citation ref `<provider>/<session-id>#<turn>` or `<source>:<provider>/<session-id>#<turn>` to the target message, optionally with context turns on each side.",
            input_schema: mcp_get_message_input_schema(),
            output_schema: Some(mcp_get_message_response_schema()),
        },
        McpToolContract {
            name: "reindex",
            description: "Refresh the search index (incremental by default). Returns counts of added/updated/unchanged sessions.",
            input_schema: mcp_reindex_input_schema(),
            output_schema: Some(mcp_reindex_response_schema()),
        },
        McpToolContract {
            name: "health",
            description: "Run the same checks as `aghist health`: provider detection, index state, source caches, metadata DB, and embedding sidecars.",
            input_schema: mcp_health_input_schema(),
            output_schema: Some(health_response_schema()),
        },
    ]
}

fn mcp_search_sessions_input_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "query",
                json!({ "type": "string", "maxLength": SEARCH_QUERY_MAX_BYTES, "description": "Tantivy query string. Matches the `content` and `project` fields." }),
            ),
            (
                "limit",
                json!({ "type": "integer", "minimum": 1, "maximum": MCP_SEARCH_LIMIT_MAX, "default": SEARCH_LIMIT_DEFAULT }),
            ),
        ]),
        &["query"],
    )
}

fn mcp_list_sessions_input_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "provider",
                json!({ "type": "string", "enum": provider_slug_enum() }),
            ),
            (
                "project",
                json!({ "type": "string", "maxLength": MCP_FILTER_STRING_MAX_BYTES, "description": "Substring match on session project_name." }),
            ),
            (
                "limit",
                json!({ "type": "integer", "minimum": 1, "maximum": MCP_LIST_LIMIT_MAX, "default": MCP_LIST_LIMIT_DEFAULT }),
            ),
        ]),
        &[],
    )
}

fn mcp_get_session_input_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "session_id",
                json!({ "type": "string", "maxLength": MCP_LOOKUP_STRING_MAX_BYTES }),
            ),
            (
                "provider",
                json!({ "type": "string", "enum": provider_slug_enum() }),
            ),
            (
                "source",
                json!({ "type": "string", "maxLength": MAX_SOURCE_NAME_BYTES, "description": "Source name from list_sessions. Omit for unique matches; use 'local' for local-only lookup." }),
            ),
        ]),
        &["session_id"],
    )
}

fn mcp_get_message_input_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "ref",
                json!({
                    "type": "string",
                    "maxLength": MCP_LOOKUP_STRING_MAX_BYTES,
                    "pattern": source_qualified_citation_ref_pattern(),
                    "description": "Citation ref. Example: claude-code/abc-123#7"
                }),
            ),
            (
                "include_context",
                json!({ "type": "integer", "minimum": 0, "maximum": MCP_INCLUDE_CONTEXT_MAX, "default": MCP_INCLUDE_CONTEXT_DEFAULT }),
            ),
        ]),
        &["ref"],
    )
}

fn mcp_reindex_input_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "provider",
                json!({ "type": "string", "enum": provider_slug_enum() }),
            ),
            (
                "force",
                json!({ "type": "boolean", "default": false, "description": "Clear the index first for a full rebuild." }),
            ),
        ]),
        &[],
    )
}

fn mcp_health_input_schema() -> Value {
    closed_empty_object_schema()
}

#[cfg(test)]
pub(crate) fn mcp_tool_output_schema(tool_name: &str) -> Option<Value> {
    mcp_tool_contracts()
        .into_iter()
        .find(|contract| contract.name == tool_name)
        .and_then(|contract| contract.output_schema)
}
