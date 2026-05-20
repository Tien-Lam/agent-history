use serde_json::{json, Value};

use super::super::super::common::{
    closed_object_schema, exit_codes, schema_props, source_qualified_session_only_ref_pattern,
    SCHEMA_DRAFT,
};

pub(in crate::schema) fn diff_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/diff",
        "title": "aghist diff",
        "command": "diff",
        "description": "Compare two sessions turn-by-turn using longest-common-subsequence over role + content snippets.",
        "params": closed_object_schema(
            schema_props([
                (
                    "session1",
                    json!({
                        "type": "string",
                        "pattern": source_qualified_session_only_ref_pattern(),
                        "description": "First session ref. Examples: claude-code/abc-123, laptop:claude-code/abc-123"
                    }),
                ),
                (
                    "session2",
                    json!({
                        "type": "string",
                        "pattern": source_qualified_session_only_ref_pattern(),
                        "description": "Second session ref. Examples: claude-code/def-456, laptop:claude-code/def-456"
                    }),
                ),
                (
                    "context",
                    json!({
                        "type": "integer",
                        "minimum": 0,
                        "default": 2,
                        "description": "Context lines around each changed hunk in text output."
                    }),
                ),
                (
                    "json",
                    json!({
                        "type": "boolean",
                        "description": "Force JSON output."
                    }),
                ),
            ]),
            &["session1", "session2"],
        ),
        "response": {
            "type": "object",
            "description": "JSON output when --json or stdout is not a TTY. Text output is unified-diff style.",
            "properties": {
                "session1": { "$ref": "#/definitions/DiffSession" },
                "session2": { "$ref": "#/definitions/DiffSession" },
                "ops": {
                    "type": "array",
                    "items": { "$ref": "#/definitions/DiffOp" }
                },
                "changed": { "type": "integer", "minimum": 0 },
                "same": { "type": "integer", "minimum": 0 }
            },
            "required": ["session1", "session2", "ops", "changed", "same"]
        },
        "definitions": {
            "DiffSession": {
                "type": "object",
                "properties": {
                    "ref": { "type": "string", "pattern": source_qualified_session_only_ref_pattern() },
                    "started_at": { "type": "string", "format": "date-time" },
                    "turns": { "type": "integer", "minimum": 0 }
                },
                "required": ["ref", "started_at", "turns"]
            },
            "DiffOp": {
                "type": "object",
                "properties": {
                    "op": { "type": "string", "enum": ["same", "delete", "insert"] },
                    "role": { "type": "string" },
                    "snippet": { "type": "string" },
                    "turn_a": { "type": "integer", "minimum": 1 },
                    "turn_b": { "type": "integer", "minimum": 1 }
                },
                "required": ["op", "role", "snippet"]
            }
        },
        "exit_codes": exit_codes()
    })
}
