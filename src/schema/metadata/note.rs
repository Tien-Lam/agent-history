use serde_json::{json, Value};

use super::super::common::{
    count_array_response, exit_codes, source_qualified_session_ref_pattern, SCHEMA_DRAFT,
};

fn note_row_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "integer", "minimum": 1 },
            "session_ref": {
                "type": "string",
                "pattern": source_qualified_session_ref_pattern(),
                "description": "<provider>/<session-id>[#<turn>] or <source>:<provider>/<session-id>[#<turn>]"
            },
            "body": { "type": "string", "minLength": 1 },
            "created_at": { "type": "string", "description": "ISO-8601 UTC, sub-second precision." },
            "updated_at": { "type": "string", "description": "ISO-8601 UTC, sub-second precision." }
        },
        "required": ["id", "session_ref", "body", "created_at", "updated_at"]
    })
}

fn note_subcommands_schema() -> Value {
    json!({
        "add": {
            "description": "Attach a new note. Body comes from --body, --body-file, or --stdin.",
            "params": {
                "type": "object",
                "properties": {
                    "reference": { "type": "string", "pattern": source_qualified_session_ref_pattern() },
                    "body": { "type": "string", "description": "Literal body text. Mutually exclusive with body_file/stdin." },
                    "body_file": { "type": "string", "description": "Path to read body from ('-' for stdin)." },
                    "stdin": { "type": "boolean", "description": "Read body from standard input." }
                },
                "required": ["reference"],
                "additionalProperties": false
            },
            "response": {
                "type": "object",
                "properties": { "added": { "$ref": "#/definitions/Note" } },
                "required": ["added"]
            }
        },
        "list": {
            "description": "List notes, optionally filtered by session ref. Session-level filter matches the session row plus all of its turns; turn-level filter matches that turn exactly.",
            "params": {
                "type": "object",
                "properties": {
                    "reference": { "type": "string", "pattern": source_qualified_session_ref_pattern() },
                    "json": { "type": "boolean" }
                },
                "additionalProperties": false
            },
            "response": count_array_response("notes", "#/definitions/Note")
        },
        "edit": {
            "description": "Replace an existing note's body. Bumps updated_at.",
            "params": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "minimum": 1 },
                    "body": { "type": "string" },
                    "body_file": { "type": "string" },
                    "stdin": { "type": "boolean" }
                },
                "required": ["id"],
                "additionalProperties": false
            },
            "response": {
                "type": "object",
                "properties": { "updated": { "$ref": "#/definitions/Note" } },
                "required": ["updated"]
            }
        },
        "remove": {
            "description": "Delete a note by id. Returns the deleted row.",
            "params": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "minimum": 1 }
                },
                "required": ["id"],
                "additionalProperties": false
            },
            "response": {
                "type": "object",
                "properties": { "removed": { "$ref": "#/definitions/Note" } },
                "required": ["removed"]
            }
        }
    })
}

pub(in crate::schema) fn note_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/note",
        "title": "aghist note",
        "command": "note",
        "description": "Manage per-user notes attached to sessions or turns. Notes live in the metadata sidecar (~/.local/share/aghist/metadata.db; AGHIST_METADATA_DB overrides). aghist never mutates provider history files.",
        "params": {
            "type": "object",
            "description": "Top-level dispatch: see `subcommands` for the per-subcommand schemas.",
            "properties": {
                "subcommand": { "type": "string", "enum": ["add", "list", "edit", "remove"] }
            },
            "required": ["subcommand"],
            "additionalProperties": false
        },
        "response": {
            "type": "object",
            "description": "Shape varies by subcommand — see `subcommands.<name>.response`."
        },
        "subcommands": note_subcommands_schema(),
        "definitions": { "Note": note_row_schema() },
        "exit_codes": exit_codes()
    })
}
