use serde_json::{json, Value};

use crate::schema_fragments;

pub(super) const SCHEMA_DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

pub(super) use schema_fragments::{
    array_schema, closed_empty_object_schema, closed_object_schema, object_schema, schema_props,
    with_description, SchemaProperties,
};

pub(super) fn schema_props_with_filters(
    entries: impl IntoIterator<Item = (&'static str, Value)>,
) -> SchemaProperties {
    let mut props = schema_props(entries);
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }
    props
}

pub(super) fn schema_ref(reference: &'static str) -> Value {
    json!({ "$ref": reference })
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

pub(super) fn todo_target_ref_pattern() -> String {
    schema_fragments::todo_target_ref_pattern()
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
                "enum": ["user", "assistant", "system", "tool"],
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
    props.insert(field.to_string(), array_schema(json!({ "$ref": item_ref })));
    props.insert(
        "count".to_string(),
        json!({ "type": "integer", "minimum": 0 }),
    );
    object_schema(props, &[field, "count"])
}
