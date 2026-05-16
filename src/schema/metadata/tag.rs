use serde_json::{json, Value};

use super::super::common::{
    count_array_response, exit_codes, source_qualified_session_ref_pattern, SCHEMA_DRAFT,
};

fn tag_row_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "integer", "minimum": 1 },
            "session_ref": {
                "type": "string",
                "pattern": source_qualified_session_ref_pattern(),
                "description": "<provider>/<session-id>[#<turn>] or <source>:<provider>/<session-id>[#<turn>]"
            },
            "tag": { "type": "string", "minLength": 1 },
            "created_at": { "type": "string", "description": "ISO-8601 UTC, sub-second precision." }
        },
        "required": ["id", "session_ref", "tag", "created_at"]
    })
}

fn tag_subcommands_schema() -> Value {
    json!({
        "add": {
            "description": "Attach a tag to a session ref. Adding the same (ref, tag) pair twice raises a `tag-conflict` error.",
            "params": {
                "type": "object",
                "properties": {
                    "reference": { "type": "string", "pattern": source_qualified_session_ref_pattern() },
                    "tag": { "type": "string", "minLength": 1 }
                },
                "required": ["reference", "tag"],
                "additionalProperties": false
            },
            "response": {
                "type": "object",
                "properties": { "added": { "$ref": "#/definitions/Tag" } },
                "required": ["added"]
            }
        },
        "list": {
            "description": "List tags, optionally filtered by session ref and/or tag value. Session-level filter matches the session row plus all of its turns; turn-level filter matches that turn exactly. `tag` filter narrows to a specific tag value and combines with the ref filter.",
            "params": {
                "type": "object",
                "properties": {
                    "reference": { "type": "string", "pattern": source_qualified_session_ref_pattern() },
                    "tag": { "type": "string", "minLength": 1 },
                    "json": { "type": "boolean" }
                },
                "additionalProperties": false
            },
            "response": count_array_response("tags", "#/definitions/Tag")
        },
        "remove": {
            "description": "Detach a tag from a session ref. Returns the deleted row, or `tag-not-found` if no matching pair exists.",
            "params": {
                "type": "object",
                "properties": {
                    "reference": { "type": "string", "pattern": source_qualified_session_ref_pattern() },
                    "tag": { "type": "string", "minLength": 1 }
                },
                "required": ["reference", "tag"],
                "additionalProperties": false
            },
            "response": {
                "type": "object",
                "properties": { "removed": { "$ref": "#/definitions/Tag" } },
                "required": ["removed"]
            }
        }
    })
}

pub(in crate::schema) fn tag_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/tag",
        "title": "aghist tag",
        "command": "tag",
        "description": "Manage per-user tags attached to sessions or turns. Tags live in the metadata sidecar (~/.local/share/aghist/metadata.db; AGHIST_METADATA_DB overrides). The (session_ref, tag) pair is unique. aghist never mutates provider history files.",
        "params": {
            "type": "object",
            "description": "Top-level dispatch: see `subcommands` for the per-subcommand schemas.",
            "properties": {
                "subcommand": { "type": "string", "enum": ["add", "list", "remove"] }
            },
            "required": ["subcommand"],
            "additionalProperties": false
        },
        "response": {
            "type": "object",
            "description": "Shape varies by subcommand — see `subcommands.<name>.response`."
        },
        "subcommands": tag_subcommands_schema(),
        "definitions": { "Tag": tag_row_schema() },
        "exit_codes": exit_codes()
    })
}
