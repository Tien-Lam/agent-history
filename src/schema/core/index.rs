use serde_json::{json, Value};

use super::super::common::{exit_codes, provider_slug_enum, SCHEMA_DRAFT};

pub(in crate::schema) fn index_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/index",
        "title": "aghist index",
        "command": "index",
        "description": "Build or refresh the search index. Idempotent and delta-aware.",
        "params": {
            "type": "object",
            "properties": {
                "provider": { "type": "string", "enum": provider_slug_enum() },
                "force": { "type": "boolean", "default": false, "description": "Clear the index before rebuilding." },
                "accept_download": { "type": "boolean", "default": false, "description": "Authorise the embedding-model download (~90 MB)." }
            },
            "additionalProperties": false
        },
        "response": {
            "type": "object",
            "properties": {
                "providers": { "type": "array", "items": { "type": "string", "enum": provider_slug_enum() } },
                "sessions_total": { "type": "integer", "minimum": 0 },
                "added": { "type": "integer", "minimum": 0 },
                "updated": { "type": "integer", "minimum": 0 },
                "unchanged": { "type": "integer", "minimum": 0 },
                "messages_indexed": { "type": "integer", "minimum": 0 },
                "force": { "type": "boolean" },
                "index_dir": { "type": "string" },
                "duration_ms": { "type": "integer", "minimum": 0 },
                "errors": { "type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "provider": { "type": "string", "enum": provider_slug_enum() },
                        "error": { "type": "string" }
                    },
                    "required": ["provider", "error"]
                } },
                "embeddings": {
                    "type": "object",
                    "description": "Status of the optional semantic-embedding pass. Shape varies by status.",
                    "properties": {
                        "status": { "type": "string", "enum": ["disabled", "awaiting-consent", "enabled"] }
                    },
                    "required": ["status"]
                }
            },
            "required": ["providers", "sessions_total", "added", "updated", "unchanged", "messages_indexed", "force", "index_dir", "duration_ms", "errors", "embeddings"]
        },
        "exit_codes": exit_codes()
    })
}
