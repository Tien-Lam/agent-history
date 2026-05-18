use serde_json::{json, Map, Value};

use crate::schema_fragments;

pub(super) const SCHEMA_DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

pub(super) type SchemaProperties = Map<String, Value>;

pub(super) fn schema_props(
    entries: impl IntoIterator<Item = (&'static str, Value)>,
) -> SchemaProperties {
    entries
        .into_iter()
        .map(|(name, schema)| (name.to_string(), schema))
        .collect()
}

pub(super) fn object_schema(properties: SchemaProperties, required: &[&str]) -> Value {
    object_schema_with(properties, required, false)
}

pub(super) fn closed_object_schema(properties: SchemaProperties, required: &[&str]) -> Value {
    object_schema_with(properties, required, true)
}

pub(super) fn closed_empty_object_schema() -> Value {
    closed_object_schema(SchemaProperties::new(), &[])
}

fn object_schema_with(properties: SchemaProperties, required: &[&str], closed: bool) -> Value {
    let mut schema = schema_props([
        ("type", json!("object")),
        ("properties", Value::Object(properties)),
    ]);
    if !required.is_empty() {
        schema.insert("required".to_string(), json!(required));
    }
    if closed {
        schema.insert("additionalProperties".to_string(), json!(false));
    }
    Value::Object(schema)
}

pub(super) fn source_qualified_session_ref_pattern() -> String {
    schema_fragments::source_qualified_session_ref_pattern()
}

pub(super) fn source_qualified_session_only_ref_pattern() -> String {
    schema_fragments::source_qualified_session_only_ref_pattern()
}

pub(super) fn source_qualified_citation_ref_pattern() -> String {
    schema_fragments::source_qualified_citation_ref_pattern()
}

pub(super) fn provider_slug_enum() -> Value {
    schema_fragments::provider_slug_enum()
}

pub(super) fn exit_codes() -> Value {
    json!({
        "0": "success with results",
        "1": "runtime error (JSON envelope on stderr)",
        "2": "usage error (bad flags or parse failure)",
        "3": "success but empty (no rows / no hits)"
    })
}

/// Filter flags shared by `--list` and `search`. Returned as a fragment so
/// each subcommand schema can fold these in alongside its own params.
pub(super) fn filter_params_fragment() -> Vec<(&'static str, Value)> {
    vec![
        (
            "provider",
            json!({
                "type": "string",
                "enum": provider_slug_enum(),
                "description": "Restrict to a single provider."
            }),
        ),
        (
            "since",
            json!({
                "type": "string",
                "format": "date-time",
                "description": "RFC 3339 lower bound on message/session timestamp (inclusive)."
            }),
        ),
        (
            "until",
            json!({
                "type": "string",
                "format": "date-time",
                "description": "RFC 3339 upper bound on message/session timestamp (inclusive)."
            }),
        ),
        (
            "project",
            json!({
                "type": "string",
                "description": "Substring match against the session's project name (case-insensitive)."
            }),
        ),
        (
            "role",
            json!({
                "type": "string",
                "enum": ["user", "assistant", "tool"],
                "description": "Restrict to messages with this role."
            }),
        ),
        (
            "has_tool_call",
            json!({
                "type": "boolean",
                "description": "Keep only messages (or sessions containing messages) with a tool invocation."
            }),
        ),
        (
            "note",
            json!({
                "type": "string",
                "description": "Keep only sessions that have a user note whose body contains this case-insensitive substring (session-level OR any of its turns). Backed by the metadata sidecar."
            }),
        ),
        (
            "tag",
            json!({
                "type": "string",
                "description": "Keep only sessions with this exact tag attached (session-level OR any of its turns). Backed by the metadata sidecar."
            }),
        ),
        (
            "starred",
            json!({
                "type": "boolean",
                "description": "Keep only sessions with at least one star (session-level OR any of its turns). Backed by the metadata sidecar."
            }),
        ),
    ]
}

pub(super) fn session_row_schema() -> Value {
    schema_fragments::session_row_schema()
}

pub(super) fn list_response_schema() -> Value {
    schema_fragments::list_response_schema()
}

pub(super) fn search_response_schema() -> Value {
    schema_fragments::search_response_schema()
}

pub(super) fn count_array_response(field: &str, item_ref: &str) -> Value {
    let mut props = serde_json::Map::new();
    props.insert(
        field.to_string(),
        json!({ "type": "array", "items": { "$ref": item_ref } }),
    );
    props.insert(
        "count".to_string(),
        json!({ "type": "integer", "minimum": 0 }),
    );
    json!({
        "type": "object",
        "properties": Value::Object(props),
        "required": [field, "count"]
    })
}
