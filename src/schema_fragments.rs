use serde_json::{json, Map, Value};

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

type SchemaProperties = Map<String, Value>;

fn schema_props(entries: impl IntoIterator<Item = (&'static str, Value)>) -> SchemaProperties {
    entries
        .into_iter()
        .map(|(name, schema)| (name.to_string(), schema))
        .collect()
}

fn closed_object_schema(properties: SchemaProperties, required: &[&str]) -> Value {
    let mut schema = schema_props([
        ("type", json!("object")),
        ("properties", Value::Object(properties)),
    ]);
    if !required.is_empty() {
        schema.insert("required".to_string(), json!(required));
    }
    schema.insert("additionalProperties".to_string(), json!(false));
    Value::Object(schema)
}

fn closed_empty_object_schema() -> Value {
    closed_object_schema(SchemaProperties::new(), &[])
}

fn array_schema(items: Value) -> Value {
    let mut schema = schema_props([("type", json!("array"))]);
    schema.insert("items".to_string(), items);
    Value::Object(schema)
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
    closed_object_schema(
        schema_props([
            (
                "query",
                json!({ "type": "string", "description": "Tantivy query string. Matches the `content` and `project` fields." }),
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
                json!({ "type": "string", "description": "Substring match on session project_name." }),
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
            ("session_id", json!({ "type": "string" })),
            (
                "provider",
                json!({ "type": "string", "enum": provider_slug_enum() }),
            ),
            (
                "source",
                json!({ "type": "string", "description": "Source name from list_sessions. Omit for unique matches; use 'local' for local-only lookup." }),
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

pub(crate) fn cursor_meta_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "next_cursor",
                json!({
                    "type": ["string", "null"],
                    "description": "Opaque pagination cursor; pass back with --cursor."
                }),
            ),
            ("total", json!({ "type": "integer", "minimum": 0 })),
        ]),
        &["next_cursor", "total"],
    )
}

pub(crate) fn session_row_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("id", json!({ "type": "string" })),
            (
                "source",
                json!({ "type": "string", "description": "`local` for this host, or a registered remote source name." }),
            ),
            (
                "provider",
                json!({ "type": "string", "enum": provider_slug_enum() }),
            ),
            ("project", json!({ "type": ["string", "null"] })),
            ("branch", json!({ "type": ["string", "null"] })),
            ("summary", json!({ "type": ["string", "null"] })),
            (
                "started_at",
                json!({ "type": "string", "format": "date-time" }),
            ),
            ("message_count", json!({ "type": "integer", "minimum": 0 })),
        ]),
        &["id", "source", "provider", "started_at", "message_count"],
    )
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
    let mut schema = closed_object_schema(
        schema_props([
            ("sessions", array_schema(session_row_schema())),
            ("meta", cursor_meta_schema()),
        ]),
        &["sessions", "meta"],
    );
    schema["description"] = json!("JSON output (when --json or stdout is not a TTY).");
    schema
}

pub(crate) fn source_error_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("source", json!({ "type": "string" })),
            ("error", json!({ "type": "string" })),
        ]),
        &["source", "error"],
    )
}

pub(crate) fn mcp_list_response_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("total", json!({ "type": "integer", "minimum": 0 })),
            ("sessions", array_schema(mcp_session_row_schema())),
            ("source_errors", array_schema(source_error_schema())),
        ]),
        &["total", "sessions", "source_errors"],
    )
}

pub(crate) fn search_meta_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "next_cursor",
                json!({
                    "type": ["string", "null"],
                    "description": "Opaque pagination cursor; pass back with --cursor."
                }),
            ),
            ("total", json!({ "type": "integer", "minimum": 0 })),
            (
                "engine",
                json!({
                    "type": "string",
                    "enum": ["lexical", "hybrid"],
                    "description": "Search engine that produced the results."
                }),
            ),
        ]),
        &["next_cursor", "total", "engine"],
    )
}

pub(crate) fn message_row_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "ref",
                json!({
                    "type": ["string", "null"],
                    "pattern": source_qualified_citation_ref_pattern()
                }),
            ),
            ("uri", json!({ "type": "string" })),
            ("source", json!({ "type": "string" })),
            ("turn", json!({ "type": "integer", "minimum": 1 })),
            ("id", json!({ "type": "string" })),
            (
                "role",
                json!({ "type": "string", "enum": ["user", "assistant", "system", "tool"] }),
            ),
            (
                "timestamp",
                json!({ "type": "string", "format": "date-time" }),
            ),
            ("model", json!({ "type": ["string", "null"] })),
            ("content", array_schema(message_content_block_schema())),
            ("is_target", json!({ "type": "boolean" })),
        ]),
        &[
            "ref",
            "uri",
            "source",
            "turn",
            "id",
            "role",
            "timestamp",
            "model",
            "content",
        ],
    )
}

fn message_content_block_schema() -> Value {
    closed_object_schema(
        schema_props([("type", json!({ "type": "string" })), ("data", json!({}))]),
        &["type"],
    )
}

pub(crate) fn mcp_get_session_response_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("session", mcp_session_row_schema()),
            ("turns", array_schema(message_row_schema())),
        ]),
        &["session", "turns"],
    )
}

pub(crate) fn mcp_get_message_response_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "ref",
                json!({
                    "type": "string",
                    "pattern": source_qualified_citation_ref_pattern()
                }),
            ),
            ("session", mcp_session_row_schema()),
            ("target_turn", json!({ "type": "integer", "minimum": 1 })),
            ("turns", array_schema(message_row_schema())),
        ]),
        &["ref", "session", "target_turn", "turns"],
    )
}

pub(crate) fn search_hit_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "kind",
                json!({
                    "type": "string",
                    "enum": ["message", "note"],
                    "description": "Whether the hit points at a session message or a metadata note."
                }),
            ),
            ("session_id", json!({ "type": "string" })),
            ("message_id", json!({ "type": "string" })),
            ("score", json!({ "type": "number" })),
            ("snippet", json!({ "type": "string" })),
            (
                "provider",
                json!({ "type": ["string", "null"], "enum": provider_slug_enum_nullable() }),
            ),
            ("project", json!({ "type": ["string", "null"] })),
            (
                "started_at",
                json!({ "type": ["string", "null"], "format": "date-time" }),
            ),
            (
                "source",
                json!({
                    "type": "string",
                    "description": "`local` for this host, or a registered remote source name."
                }),
            ),
            (
                "note_id",
                json!({
                    "type": "integer",
                    "description": "Present for note hits; absent for message hits."
                }),
            ),
            (
                "ref",
                json!({
                    "type": "string",
                    "pattern": source_qualified_session_ref_pattern(),
                    "description": "Citation ref for message hits, or the note's stored session ref for note hits."
                }),
            ),
            (
                "turn",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "description": "Present when the hit has been resolved to a 1-based turn number."
                }),
            ),
            (
                "explanation",
                json!({
                    "type": "object",
                    "description": "Present only with --debug-search; Tantivy score explanation tree."
                }),
            ),
        ]),
        &[
            "kind",
            "session_id",
            "message_id",
            "score",
            "snippet",
            "provider",
            "project",
            "started_at",
            "source",
        ],
    )
}

pub(crate) fn search_response_schema() -> Value {
    let mut schema = closed_object_schema(
        schema_props([
            (
                "hits",
                json!({
                    "type": "array",
                    "description": "Hits ordered by score descending then started_at descending.",
                    "items": search_hit_schema()
                }),
            ),
            ("meta", search_meta_schema()),
        ]),
        &["hits", "meta"],
    );
    schema["description"] =
        json!("JSON envelope emitted by `aghist search --json`; watch mode emits one hit object per NDJSON line.");
    schema
}

pub(crate) fn mcp_search_response_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("query", json!({ "type": "string" })),
            ("limit", json!({ "type": "integer", "minimum": 1 })),
            ("total", json!({ "type": "integer", "minimum": 0 })),
            ("hits", array_schema(search_hit_schema())),
            ("source_errors", array_schema(source_error_schema())),
        ]),
        &["query", "limit", "total", "hits", "source_errors"],
    )
}

pub(crate) fn mcp_tool_output_schema(tool_name: &str) -> Option<Value> {
    mcp_tool_contracts()
        .into_iter()
        .find(|contract| contract.name == tool_name)
        .and_then(|contract| contract.output_schema)
}
