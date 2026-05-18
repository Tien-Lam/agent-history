use serde_json::{json, Value};

use super::super::common::{
    closed_object_schema, exit_codes, object_schema, provider_slug_enum, schema_props, SCHEMA_DRAFT,
};

fn index_params_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "provider",
                json!({ "type": "string", "enum": provider_slug_enum() }),
            ),
            (
                "force",
                json!({ "type": "boolean", "default": false, "description": "Clear the index before rebuilding." }),
            ),
            (
                "accept_download",
                json!({ "type": "boolean", "default": false, "description": "Authorise the embedding-model download (~90 MB)." }),
            ),
        ]),
        &[],
    )
}

fn index_error_schema() -> Value {
    json!({
        "oneOf": [
            closed_object_schema(
                schema_props([
                    ("provider", json!({ "type": "string", "enum": provider_slug_enum() })),
                    ("error", json!({ "type": "string" })),
                ]),
                &["provider", "error"],
            ),
            closed_object_schema(
                schema_props([
                    ("source", json!({ "type": "string" })),
                    ("error", json!({ "type": "string" })),
                ]),
                &["source", "error"],
            )
        ]
    })
}

fn embeddings_status_schema() -> Value {
    object_schema(
        schema_props([(
            "status",
            json!({ "type": "string", "enum": ["disabled", "awaiting-consent", "enabled"] }),
        )]),
        &["status"],
    )
}

fn index_response_schema() -> Value {
    object_schema(
        schema_props([
            (
                "providers",
                json!({ "type": "array", "items": { "type": "string", "enum": provider_slug_enum() } }),
            ),
            ("sessions_total", json!({ "type": "integer", "minimum": 0 })),
            ("added", json!({ "type": "integer", "minimum": 0 })),
            ("updated", json!({ "type": "integer", "minimum": 0 })),
            ("unchanged", json!({ "type": "integer", "minimum": 0 })),
            ("removed", json!({ "type": "integer", "minimum": 0 })),
            (
                "messages_indexed",
                json!({ "type": "integer", "minimum": 0 }),
            ),
            ("force", json!({ "type": "boolean" })),
            ("index_dir", json!({ "type": "string" })),
            ("duration_ms", json!({ "type": "integer", "minimum": 0 })),
            (
                "errors",
                json!({ "type": "array", "items": index_error_schema() }),
            ),
            (
                "embeddings",
                with_description(
                    embeddings_status_schema(),
                    "Status of the optional semantic-embedding pass. Shape varies by status.",
                ),
            ),
        ]),
        &[
            "providers",
            "sessions_total",
            "added",
            "updated",
            "unchanged",
            "removed",
            "messages_indexed",
            "force",
            "index_dir",
            "duration_ms",
            "errors",
            "embeddings",
        ],
    )
}

fn with_description(mut schema: Value, description: &str) -> Value {
    schema["description"] = json!(description);
    schema
}

pub(in crate::schema) fn index_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/index",
        "title": "aghist index",
        "command": "index",
        "description": "Build or refresh the search index. Idempotent and delta-aware.",
        "params": index_params_schema(),
        "response": index_response_schema(),
        "exit_codes": exit_codes()
    })
}
