use serde_json::{json, Value};

use super::super::common::{array_schema, closed_object_schema, provider_slug_enum, schema_props};

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
