use serde_json::{json, Value};

use super::common::{
    array_schema, closed_object_schema, provider_slug_enum, provider_slug_enum_nullable,
    schema_props, source_qualified_citation_ref_pattern, source_qualified_session_ref_pattern,
};

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
    if let Some(properties) = schema
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
    {
        properties.insert(
            "uri".to_string(),
            json!({ "type": "string", "description": "MCP resource URI for this session." }),
        );
        properties.insert("model".to_string(), json!({ "type": ["string", "null"] }));
        properties.insert(
            "ended_at".to_string(),
            json!({ "type": ["string", "null"], "format": "date-time" }),
        );
    }
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
