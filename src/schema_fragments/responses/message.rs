use serde_json::{json, Value};

use super::super::common::{
    array_schema, closed_object_schema, schema_props, source_qualified_citation_ref_pattern,
};
use super::list::mcp_session_row_schema;

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
