use serde_json::{json, Value};

use super::super::common::{
    closed_object_schema, provider_slug_enum, schema_props, SchemaProperties,
};

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
    json!({
        "oneOf": [
            closed_object_schema(
                schema_props([
                    ("status", json!({ "const": "disabled" })),
                    ("reason", json!({ "type": "string" })),
                    ("accept_download_requested", json!({ "type": "boolean" })),
                ]),
                &["status", "reason", "accept_download_requested"],
            ),
            closed_object_schema(
                schema_props([
                    ("status", json!({ "const": "awaiting-consent" })),
                    ("model", json!({ "type": "string" })),
                    ("hint", json!({ "type": "string" })),
                ]),
                &["status", "model", "hint"],
            ),
            closed_object_schema(
                schema_props([
                    ("status", json!({ "const": "enabled" })),
                    ("model", json!({ "type": "string" })),
                    ("dim", json!({ "type": "integer", "minimum": 1 })),
                    ("messages_embedded", json!({ "type": "integer", "minimum": 0 })),
                    ("messages_reused_from_cache", json!({ "type": "integer", "minimum": 0 })),
                    ("messages_pruned_from_store", json!({ "type": "integer", "minimum": 0 })),
                    ("messages_total_in_store", json!({ "type": "integer", "minimum": 0 })),
                    ("evicted_old_schema", json!({ "type": "boolean" })),
                    ("consent_accepted_at", json!({ "type": "string", "format": "date-time" })),
                    ("errors", json!({ "type": "array", "items": { "type": "string" } })),
                ]),
                &[
                    "status",
                    "model",
                    "dim",
                    "messages_embedded",
                    "messages_reused_from_cache",
                    "messages_pruned_from_store",
                    "messages_total_in_store",
                    "evicted_old_schema",
                    "consent_accepted_at",
                    "errors",
                ],
            ),
        ]
    })
}

fn indexing_summary_schema_parts() -> (SchemaProperties, Vec<&'static str>) {
    (
        schema_props([
            (
                "status",
                json!({ "type": "string", "enum": ["ok", "partial"] }),
            ),
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
        ]),
        vec![
            "status",
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
        ],
    )
}

fn indexing_summary_response_schema() -> Value {
    let (properties, required) = indexing_summary_schema_parts();
    closed_object_schema(properties, &required)
}

pub(crate) fn index_response_schema() -> Value {
    let (mut properties, mut required) = indexing_summary_schema_parts();
    properties.insert(
        "embeddings".to_string(),
        with_description(
            embeddings_status_schema(),
            "Status of the optional semantic-embedding pass. Shape varies by status.",
        ),
    );
    required.push("embeddings");
    closed_object_schema(properties, &required)
}

pub(crate) fn mcp_reindex_response_schema() -> Value {
    indexing_summary_response_schema()
}

fn with_description(mut schema: Value, description: &str) -> Value {
    schema["description"] = json!(description);
    schema
}
