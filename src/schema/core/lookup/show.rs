use serde_json::{json, Value};

use crate::schema_fragments::{SHOW_INCLUDE_CONTEXT_DEFAULT, SHOW_INCLUDE_CONTEXT_MAX};

use super::super::super::common::{
    closed_object_schema, exit_codes, provider_slug_enum, schema_props,
    source_qualified_citation_ref_pattern, SCHEMA_DRAFT,
};

pub(in crate::schema) fn show_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/show",
        "title": "aghist show",
        "command": "show",
        "description": "Resolve a citation ref `<provider>/<session-id>#<turn>` or `<source>:<provider>/<session-id>#<turn>` to a single message.",
        "params": closed_object_schema(
            schema_props([
                (
                    "reference",
                    json!({
                        "type": "string",
                        "pattern": source_qualified_citation_ref_pattern(),
                        "description": "Citation ref. Examples: claude-code/abc-123#7, laptop:claude-code/abc-123#7"
                    }),
                ),
                (
                    "format",
                    json!({
                        "type": "string",
                        "enum": ["md", "json", "text"],
                        "default": "md"
                    }),
                ),
                (
                    "include_context",
                    json!({
                        "type": "integer",
                        "minimum": 0,
                        "maximum": SHOW_INCLUDE_CONTEXT_MAX,
                        "default": SHOW_INCLUDE_CONTEXT_DEFAULT,
                        "description": "Number of turns before and after the target to include."
                    }),
                ),
            ]),
            &["reference"],
        ),
        "response": {
            "type": "object",
            "description": "JSON output (when --format=json). Other formats emit text/markdown.",
            "properties": {
                "ref": { "type": "string" },
                "provider": { "type": "string", "enum": provider_slug_enum() },
                "session_id": { "type": "string" },
                "project": { "type": ["string", "null"] },
                "target_turn": { "type": "integer", "minimum": 1 },
                "messages": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "turn": { "type": "integer", "minimum": 1 },
                            "is_target": { "type": "boolean" },
                            "role": { "type": "string" },
                            "content": { "type": "array" },
                            "timestamp": { "type": "string", "format": "date-time" }
                        },
                        "required": ["turn", "is_target", "role", "content", "timestamp"]
                    }
                }
            },
            "required": ["ref", "provider", "session_id", "target_turn", "messages"]
        },
        "exit_codes": exit_codes()
    })
}
