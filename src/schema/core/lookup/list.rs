use serde_json::{json, Value};

use crate::schema_fragments::{CURSOR_TOKEN_MAX_BYTES, LIST_LIMIT_DEFAULT, LIST_LIMIT_MAX};

use super::super::super::common::{
    closed_object_schema, exit_codes, filter_params_fragment, list_response_schema, schema_props,
    session_row_schema, SchemaProperties, SCHEMA_DRAFT,
};

fn list_params_properties() -> SchemaProperties {
    let mut props = schema_props([
        (
            "json",
            json!({ "type": "boolean", "description": "Force JSON output (single object with `sessions` array)." }),
        ),
        (
            "ndjson",
            json!({ "type": "boolean", "description": "Force NDJSON output (one session per line)." }),
        ),
        (
            "limit",
            json!({ "type": "integer", "minimum": 1, "maximum": LIST_LIMIT_MAX, "default": LIST_LIMIT_DEFAULT }),
        ),
        (
            "cursor",
            json!({ "type": "string", "maxLength": CURSOR_TOKEN_MAX_BYTES, "description": "Opaque pagination cursor from a prior `meta.next_cursor`." }),
        ),
    ]);
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }
    props
}

pub(in crate::schema) fn list_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/list",
        "title": "aghist --list",
        "command": "--list",
        "description": "List sessions across enabled providers, sorted by start time descending.",
        "params": closed_object_schema(list_params_properties(), &[]),
        "response": {
            "oneOf": [
                list_response_schema(),
                {
                    "type": "object",
                    "description": "NDJSON output (one session per line) — each line matches this shape.",
                    "$ref": "#/definitions/SessionRow"
                }
            ]
        },
        "definitions": { "SessionRow": session_row_schema() },
        "exit_codes": exit_codes()
    })
}
