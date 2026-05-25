use serde_json::{json, Value};

use crate::schema_fragments::REFERENCE_MAX_BYTES;

use super::super::common::{
    closed_object_schema, count_array_response, exit_codes, object_schema, schema_props,
    schema_ref, source_qualified_session_ref_pattern, SCHEMA_DRAFT,
};

fn session_ref_param_schema() -> Value {
    json!({
        "type": "string",
        "maxLength": REFERENCE_MAX_BYTES,
        "pattern": source_qualified_session_ref_pattern()
    })
}

fn star_row() -> Value {
    object_schema(
        schema_props([
            (
                "session_ref",
                json!({
                    "type": "string",
                    "pattern": source_qualified_session_ref_pattern(),
                    "description": "<provider>/<session-id>[#<turn>] or <source>:<provider>/<session-id>[#<turn>]"
                }),
            ),
            (
                "starred_at",
                json!({
                    "type": "string",
                    "description": "ISO-8601 UTC, sub-second precision."
                }),
            ),
        ]),
        &["session_ref", "starred_at"],
    )
}

pub(in crate::schema) fn star_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/star",
        "title": "aghist star",
        "command": "star",
        "description": "Mark a session or turn as starred. Stars live in the metadata sidecar (~/.local/share/aghist/metadata.db; AGHIST_METADATA_DB overrides). Starring an already-starred ref raises a `star-conflict` error. aghist never mutates provider history files.",
        "params": reference_params_schema(),
        "response": star_row_response_schema("starred"),
        "definitions": { "Star": star_row() },
        "exit_codes": exit_codes()
    })
}

pub(in crate::schema) fn unstar_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/unstar",
        "title": "aghist unstar",
        "command": "unstar",
        "description": "Remove a star from a session or turn. Errors with `star-not-found` if the ref is not currently starred.",
        "params": reference_params_schema(),
        "response": star_row_response_schema("unstarred"),
        "definitions": { "Star": star_row() },
        "exit_codes": exit_codes()
    })
}

pub(in crate::schema) fn stars_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/stars",
        "title": "aghist stars",
        "command": "stars",
        "description": "List starred sessions and turns. With no ref: every star, newest first. With a session ref (optionally `<source>:`-qualified): the session row plus any of its turns. With a turn-level ref: that turn exactly. Empty result exits 3.",
        "params": closed_object_schema(
            schema_props([
                ("reference", session_ref_param_schema()),
                ("json", json!({ "type": "boolean" })),
            ]),
            &[],
        ),
        "response": count_array_response("stars", "#/definitions/Star"),
        "definitions": { "Star": star_row() },
        "exit_codes": exit_codes()
    })
}

fn reference_params_schema() -> Value {
    closed_object_schema(
        schema_props([("reference", session_ref_param_schema())]),
        &["reference"],
    )
}

fn star_row_response_schema(field: &'static str) -> Value {
    object_schema(
        schema_props([(field, schema_ref("#/definitions/Star"))]),
        &[field],
    )
}
