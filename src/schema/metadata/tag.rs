use serde_json::{json, Value};

use crate::schema_fragments::METADATA_TAG_MAX_BYTES;

use super::super::common::{
    closed_object_schema, count_array_response, exit_codes, object_schema, schema_props,
    schema_ref, source_qualified_session_ref_pattern, with_description, SCHEMA_DRAFT,
};

fn session_ref_param_schema() -> Value {
    json!({ "type": "string", "pattern": source_qualified_session_ref_pattern() })
}

fn tag_row_schema() -> Value {
    object_schema(
        schema_props([
            ("id", json!({ "type": "integer", "minimum": 1 })),
            (
                "session_ref",
                json!({
                    "type": "string",
                    "pattern": source_qualified_session_ref_pattern(),
                    "description": "<provider>/<session-id>[#<turn>] or <source>:<provider>/<session-id>[#<turn>]"
                }),
            ),
            ("tag", tag_value_schema()),
            (
                "created_at",
                json!({ "type": "string", "description": "ISO-8601 UTC, sub-second precision." }),
            ),
        ]),
        &["id", "session_ref", "tag", "created_at"],
    )
}

fn tag_subcommands_schema() -> Value {
    json!({
        "add": {
            "description": "Attach a tag to a session ref. Adding the same (ref, tag) pair twice raises a `tag-conflict` error.",
            "params": tag_mutation_params_schema(),
            "response": tag_row_response_schema("added")
        },
        "list": {
            "description": "List tags, optionally filtered by session ref and/or tag value. Session-level filter matches the session row plus all of its turns; turn-level filter matches that turn exactly. `tag` filter narrows to a specific tag value and combines with the ref filter.",
            "params": closed_object_schema(schema_props([
                ("reference", session_ref_param_schema()),
                ("tag", tag_value_schema()),
                ("json", json!({ "type": "boolean" }))
            ]), &[]),
            "response": count_array_response("tags", "#/definitions/Tag")
        },
        "remove": {
            "description": "Detach a tag from a session ref. Returns the deleted row, or `tag-not-found` if no matching pair exists.",
            "params": tag_mutation_params_schema(),
            "response": tag_row_response_schema("removed")
        }
    })
}

fn tag_mutation_params_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("reference", session_ref_param_schema()),
            ("tag", tag_value_schema()),
        ]),
        &["reference", "tag"],
    )
}

fn tag_value_schema() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": METADATA_TAG_MAX_BYTES
    })
}

fn tag_row_response_schema(field: &'static str) -> Value {
    object_schema(
        schema_props([(field, schema_ref("#/definitions/Tag"))]),
        &[field],
    )
}

pub(in crate::schema) fn tag_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/tag",
        "title": "aghist tag",
        "command": "tag",
        "description": "Manage per-user tags attached to sessions or turns. Tags live in the metadata sidecar (~/.local/share/aghist/metadata.db; AGHIST_METADATA_DB overrides). The (session_ref, tag) pair is unique. aghist never mutates provider history files.",
        "params": top_level_tag_params_schema(),
        "response": {
            "type": "object",
            "description": "Shape varies by subcommand — see `subcommands.<name>.response`."
        },
        "subcommands": tag_subcommands_schema(),
        "definitions": { "Tag": tag_row_schema() },
        "exit_codes": exit_codes()
    })
}

fn top_level_tag_params_schema() -> Value {
    let schema = closed_object_schema(
        schema_props([(
            "subcommand",
            json!({ "type": "string", "enum": ["add", "list", "remove"] }),
        )]),
        &["subcommand"],
    );
    with_description(
        schema,
        "Top-level dispatch: see `subcommands` for the per-subcommand schemas.",
    )
}
