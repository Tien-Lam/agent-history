use serde_json::{json, Value};

use crate::model::Provider;

pub(super) const SCHEMA_DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";
pub(super) const SOURCE_QUALIFIED_SESSION_REF_PATTERN: &str =
    "^([A-Za-z0-9][A-Za-z0-9_-]*:)?(claude-code|copilot-cli|gemini-cli|codex-cli|opencode|cursor)/[^#]+(#[1-9][0-9]*)?$";

pub(super) fn provider_slug_enum() -> Value {
    json!(Provider::all().iter().map(|p| p.slug()).collect::<Vec<_>>())
}

pub(super) fn provider_slug_enum_nullable() -> Value {
    let mut slugs: Vec<Value> = Provider::all().iter().map(|p| json!(p.slug())).collect();
    slugs.push(Value::Null);
    Value::Array(slugs)
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
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "source": { "type": "string", "description": "`local` for this host, or a registered remote source name." },
            "provider": { "type": "string", "enum": provider_slug_enum() },
            "project": { "type": ["string", "null"] },
            "branch": { "type": ["string", "null"] },
            "summary": { "type": ["string", "null"] },
            "started_at": { "type": "string", "format": "date-time" },
            "message_count": { "type": "integer", "minimum": 0 }
        },
        "required": ["id", "source", "provider", "started_at", "message_count"]
    })
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
