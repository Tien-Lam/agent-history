use serde_json::{json, Value};

use crate::model::Provider;

pub const LIST_LIMIT_DEFAULT: usize = 20;
pub const SEARCH_LIMIT_DEFAULT: usize = 20;
pub const SEARCH_WATCH_INTERVAL_MS_DEFAULT: u64 = 2_000;
pub const SEARCH_WATCH_ITERATIONS_DEFAULT: u32 = 0;
pub const SEARCH_HYBRID_WEIGHT_DEFAULT: f32 = 0.0;
pub const SHOW_INCLUDE_CONTEXT_DEFAULT: u32 = 0;

pub const MCP_SEARCH_LIMIT_MAX: usize = 200;
pub const MCP_LIST_LIMIT_DEFAULT: usize = 50;
pub const MCP_LIST_LIMIT_MAX: usize = 1_000;
pub const MCP_INCLUDE_CONTEXT_DEFAULT: usize = 0;
pub const MCP_INCLUDE_CONTEXT_MAX: usize = 100;

fn provider_slug_pattern() -> String {
    Provider::all()
        .iter()
        .map(|provider| provider.slug())
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn source_qualified_session_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+(#[1-9][0-9]*)?$",
        provider_slug_pattern()
    )
}

pub(crate) fn source_qualified_session_only_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+$",
        provider_slug_pattern()
    )
}

pub(crate) fn source_qualified_citation_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+#[1-9][0-9]*$",
        provider_slug_pattern()
    )
}

pub(crate) fn provider_slug_enum() -> Value {
    json!(Provider::all().iter().map(|p| p.slug()).collect::<Vec<_>>())
}

pub(crate) fn provider_slug_enum_nullable() -> Value {
    let mut slugs: Vec<Value> = Provider::all().iter().map(|p| json!(p.slug())).collect();
    slugs.push(Value::Null);
    Value::Array(slugs)
}

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
            output_schema: None,
        },
        McpToolContract {
            name: "health",
            description: "Run the same checks as `aghist health`: provider detection, index dir writability, manifest sanity, schema presence.",
            input_schema: mcp_health_input_schema(),
            output_schema: None,
        },
    ]
}

fn mcp_search_sessions_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": { "type": "string", "description": "Tantivy query string. Matches the `content` and `project` fields." },
            "limit": { "type": "integer", "minimum": 1, "maximum": MCP_SEARCH_LIMIT_MAX, "default": SEARCH_LIMIT_DEFAULT }
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

fn mcp_list_sessions_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "provider": { "type": "string", "enum": provider_slug_enum() },
            "project": { "type": "string", "description": "Substring match on session project_name." },
            "limit": { "type": "integer", "minimum": 1, "maximum": MCP_LIST_LIMIT_MAX, "default": MCP_LIST_LIMIT_DEFAULT }
        },
        "additionalProperties": false
    })
}

fn mcp_get_session_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string" },
            "provider": { "type": "string", "enum": provider_slug_enum() },
            "source": { "type": "string", "description": "Source name from list_sessions. Omit for unique matches; use 'local' for local-only lookup." }
        },
        "required": ["session_id"],
        "additionalProperties": false
    })
}

fn mcp_get_message_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "ref": {
                "type": "string",
                "pattern": source_qualified_citation_ref_pattern(),
                "description": "Citation ref. Example: claude-code/abc-123#7"
            },
            "include_context": { "type": "integer", "minimum": 0, "maximum": MCP_INCLUDE_CONTEXT_MAX, "default": MCP_INCLUDE_CONTEXT_DEFAULT }
        },
        "required": ["ref"],
        "additionalProperties": false
    })
}

fn mcp_reindex_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "provider": { "type": "string", "enum": provider_slug_enum() },
            "force": { "type": "boolean", "default": false, "description": "Clear the index first for a full rebuild." }
        },
        "additionalProperties": false
    })
}

fn mcp_health_input_schema() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

pub(crate) fn cursor_meta_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "next_cursor": {
                "type": ["string", "null"],
                "description": "Opaque pagination cursor; pass back with --cursor."
            },
            "total": { "type": "integer", "minimum": 0 }
        },
        "required": ["next_cursor", "total"],
        "additionalProperties": false
    })
}

pub(crate) fn session_row_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "source": { "type": "string", "description": "`local` for this host, or a registered remote source name." },
            "provider": { "type": "string", "enum": provider_slug_enum() },
            "project": { "type": ["string", "null"] },
            "branch": { "type": ["string", "null"] },
            "summary": { "type": ["string", "null"] },
            "started_at": { "type": "string", "format": "date-time" },
            "message_count": { "type": "integer", "minimum": 0 }
        },
        "required": ["id", "source", "provider", "started_at", "message_count"],
        "additionalProperties": false
    })
}

pub(crate) fn mcp_session_row_schema() -> Value {
    let mut schema = session_row_schema();
    let properties = schema["properties"]
        .as_object_mut()
        .expect("session row schema properties");
    properties.insert(
        "uri".to_string(),
        json!({ "type": "string", "description": "MCP resource URI for this session." }),
    );
    properties.insert("model".to_string(), json!({ "type": ["string", "null"] }));
    properties.insert(
        "ended_at".to_string(),
        json!({ "type": ["string", "null"], "format": "date-time" }),
    );
    schema
}

pub(crate) fn list_response_schema() -> Value {
    json!({
        "type": "object",
        "description": "JSON output (when --json or stdout is not a TTY).",
        "properties": {
            "sessions": { "type": "array", "items": session_row_schema() },
            "meta": cursor_meta_schema()
        },
        "required": ["sessions", "meta"],
        "additionalProperties": false
    })
}

pub(crate) fn source_error_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "source": { "type": "string" },
            "error": { "type": "string" }
        },
        "required": ["source", "error"],
        "additionalProperties": false
    })
}

pub(crate) fn mcp_list_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "total": { "type": "integer", "minimum": 0 },
            "sessions": { "type": "array", "items": mcp_session_row_schema() },
            "source_errors": { "type": "array", "items": source_error_schema() }
        },
        "required": ["total", "sessions", "source_errors"],
        "additionalProperties": false
    })
}

pub(crate) fn search_meta_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "next_cursor": {
                "type": ["string", "null"],
                "description": "Opaque pagination cursor; pass back with --cursor."
            },
            "total": { "type": "integer", "minimum": 0 },
            "engine": {
                "type": "string",
                "enum": ["lexical", "hybrid"],
                "description": "Search engine that produced the results."
            }
        },
        "required": ["next_cursor", "total", "engine"],
        "additionalProperties": false
    })
}

pub(crate) fn message_row_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "ref": {
                "type": ["string", "null"],
                "pattern": source_qualified_citation_ref_pattern()
            },
            "uri": { "type": "string" },
            "source": { "type": "string" },
            "turn": { "type": "integer", "minimum": 1 },
            "id": { "type": "string" },
            "role": {
                "type": "string",
                "enum": ["user", "assistant", "system", "tool"]
            },
            "timestamp": { "type": "string", "format": "date-time" },
            "model": { "type": ["string", "null"] },
            "content": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "type": { "type": "string" },
                        "data": {}
                    },
                    "required": ["type"],
                    "additionalProperties": false
                }
            },
            "is_target": { "type": "boolean" }
        },
        "required": ["ref", "uri", "source", "turn", "id", "role", "timestamp", "model", "content"],
        "additionalProperties": false
    })
}

pub(crate) fn mcp_get_session_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session": mcp_session_row_schema(),
            "turns": { "type": "array", "items": message_row_schema() }
        },
        "required": ["session", "turns"],
        "additionalProperties": false
    })
}

pub(crate) fn mcp_get_message_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "ref": {
                "type": "string",
                "pattern": source_qualified_citation_ref_pattern()
            },
            "session": mcp_session_row_schema(),
            "target_turn": { "type": "integer", "minimum": 1 },
            "turns": { "type": "array", "items": message_row_schema() }
        },
        "required": ["ref", "session", "target_turn", "turns"],
        "additionalProperties": false
    })
}

pub(crate) fn search_hit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["message", "note"],
                "description": "Whether the hit points at a session message or a metadata note."
            },
            "session_id": { "type": "string" },
            "message_id": { "type": "string" },
            "score": { "type": "number" },
            "snippet": { "type": "string" },
            "provider": { "type": ["string", "null"], "enum": provider_slug_enum_nullable() },
            "project": { "type": ["string", "null"] },
            "started_at": { "type": ["string", "null"], "format": "date-time" },
            "source": {
                "type": "string",
                "description": "`local` for this host, or a registered remote source name."
            },
            "note_id": {
                "type": "integer",
                "description": "Present for note hits; absent for message hits."
            },
            "ref": {
                "type": "string",
                "pattern": source_qualified_session_ref_pattern(),
                "description": "Citation ref for message hits, or the note's stored session ref for note hits."
            },
            "turn": {
                "type": "integer",
                "minimum": 1,
                "description": "Present when the hit has been resolved to a 1-based turn number."
            },
            "explanation": {
                "type": "object",
                "description": "Present only with --debug-search; Tantivy score explanation tree."
            }
        },
        "required": [
            "kind",
            "session_id",
            "message_id",
            "score",
            "snippet",
            "provider",
            "project",
            "started_at",
            "source"
        ],
        "additionalProperties": false
    })
}

pub(crate) fn search_response_schema() -> Value {
    json!({
        "type": "object",
        "description": "JSON envelope emitted by `aghist search --json`; watch mode emits one hit object per NDJSON line.",
        "properties": {
            "hits": {
                "type": "array",
                "description": "Hits ordered by score descending then started_at descending.",
                "items": search_hit_schema()
            },
            "meta": search_meta_schema()
        },
        "required": ["hits", "meta"],
        "additionalProperties": false
    })
}

pub(crate) fn mcp_search_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": { "type": "string" },
            "limit": { "type": "integer", "minimum": 1 },
            "total": { "type": "integer", "minimum": 0 },
            "hits": { "type": "array", "items": search_hit_schema() },
            "source_errors": { "type": "array", "items": source_error_schema() }
        },
        "required": ["query", "limit", "total", "hits", "source_errors"],
        "additionalProperties": false
    })
}

pub(crate) fn mcp_tool_output_schema(tool_name: &str) -> Option<Value> {
    mcp_tool_contracts()
        .into_iter()
        .find(|contract| contract.name == tool_name)
        .and_then(|contract| contract.output_schema)
}
