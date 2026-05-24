use serde_json::{json, Value};

use super::super::common::{
    closed_object_schema, count_array_response, exit_codes, object_schema, schema_props,
    schema_ref, source_qualified_session_ref_pattern, with_description, SCHEMA_DRAFT,
};

fn session_ref_param_schema() -> Value {
    json!({ "type": "string", "pattern": source_qualified_session_ref_pattern() })
}

fn note_row_schema() -> Value {
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
            ("body", json!({ "type": "string", "minLength": 1 })),
            (
                "created_at",
                json!({ "type": "string", "description": "ISO-8601 UTC, sub-second precision." }),
            ),
            (
                "updated_at",
                json!({ "type": "string", "description": "ISO-8601 UTC, sub-second precision." }),
            ),
        ]),
        &["id", "session_ref", "body", "created_at", "updated_at"],
    )
}

fn note_subcommands_schema() -> Value {
    json!({
        "add": {
            "description": "Attach a new note. Body comes from --body, --body-file, or --stdin.",
            "params": closed_object_schema(
                schema_props([
                    ("reference", session_ref_param_schema()),
                    ("body", json!({ "type": "string", "description": "Literal body text. Mutually exclusive with body_file/stdin." })),
                    ("body_file", json!({ "type": "string", "description": "Path to read body from ('-' for stdin)." })),
                    ("stdin", json!({ "type": "boolean", "description": "Read body from standard input." })),
                ]),
                &["reference"],
            ),
            "response": note_row_response_schema("added")
        },
        "list": {
            "description": "List notes, optionally filtered by session ref. Session-level filter matches the session row plus all of its turns; turn-level filter matches that turn exactly.",
            "params": closed_object_schema(
                schema_props([
                    ("reference", session_ref_param_schema()),
                    ("json", json!({ "type": "boolean" })),
                ]),
                &[],
            ),
            "response": count_array_response("notes", "#/definitions/Note")
        },
        "edit": {
            "description": "Replace an existing note's body. Bumps updated_at.",
            "params": closed_object_schema(
                schema_props([
                    ("id", json!({ "type": "integer", "minimum": 1 })),
                    ("body", json!({ "type": "string" })),
                    ("body_file", json!({ "type": "string" })),
                    ("stdin", json!({ "type": "boolean" })),
                ]),
                &["id"],
            ),
            "response": note_row_response_schema("updated")
        },
        "remove": {
            "description": "Delete a note by id. Returns the deleted row.",
            "params": closed_object_schema(
                schema_props([("id", json!({ "type": "integer", "minimum": 1 }))]),
                &["id"],
            ),
            "response": note_row_response_schema("removed")
        }
    })
}

fn note_row_response_schema(field: &'static str) -> Value {
    object_schema(
        schema_props([(field, schema_ref("#/definitions/Note"))]),
        &[field],
    )
}

pub(in crate::schema) fn note_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/note",
        "title": "aghist note",
        "command": "note",
        "description": "Manage per-user notes attached to sessions or turns. Notes live in the metadata sidecar (~/.local/share/aghist/metadata.db; AGHIST_METADATA_DB overrides). aghist never mutates provider history files.",
        "params": top_level_note_params_schema(),
        "response": {
            "type": "object",
            "description": "Shape varies by subcommand — see `subcommands.<name>.response`."
        },
        "subcommands": note_subcommands_schema(),
        "definitions": { "Note": note_row_schema() },
        "exit_codes": exit_codes()
    })
}

fn top_level_note_params_schema() -> Value {
    let schema = closed_object_schema(
        schema_props([(
            "subcommand",
            json!({ "type": "string", "enum": ["add", "list", "edit", "remove"] }),
        )]),
        &["subcommand"],
    );
    with_description(
        schema,
        "Top-level dispatch: see `subcommands` for the per-subcommand schemas.",
    )
}
