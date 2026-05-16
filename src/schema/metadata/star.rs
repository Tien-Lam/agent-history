use serde_json::{json, Value};

use super::super::common::{exit_codes, source_qualified_session_ref_pattern, SCHEMA_DRAFT};

fn star_row() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_ref": {
                "type": "string",
                "pattern": source_qualified_session_ref_pattern(),
                "description": "<provider>/<session-id>[#<turn>] or <source>:<provider>/<session-id>[#<turn>]"
            },
            "starred_at": {
                "type": "string",
                "description": "ISO-8601 UTC, sub-second precision."
            }
        },
        "required": ["session_ref", "starred_at"]
    })
}

pub(in crate::schema) fn star_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/star",
        "title": "aghist star",
        "command": "star",
        "description": "Mark a session or turn as starred. Stars live in the metadata sidecar (~/.local/share/aghist/metadata.db; AGHIST_METADATA_DB overrides). Starring an already-starred ref raises a `star-conflict` error. aghist never mutates provider history files.",
        "params": {
            "type": "object",
            "properties": {
                "reference": { "type": "string", "pattern": source_qualified_session_ref_pattern() }
            },
            "required": ["reference"],
            "additionalProperties": false
        },
        "response": {
            "type": "object",
            "properties": { "starred": { "$ref": "#/definitions/Star" } },
            "required": ["starred"]
        },
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
        "params": {
            "type": "object",
            "properties": {
                "reference": { "type": "string", "pattern": source_qualified_session_ref_pattern() }
            },
            "required": ["reference"],
            "additionalProperties": false
        },
        "response": {
            "type": "object",
            "properties": { "unstarred": { "$ref": "#/definitions/Star" } },
            "required": ["unstarred"]
        },
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
        "params": {
            "type": "object",
            "properties": {
                "reference": { "type": "string", "pattern": source_qualified_session_ref_pattern() },
                "json": { "type": "boolean" }
            },
            "additionalProperties": false
        },
        "response": {
            "type": "object",
            "properties": {
                "stars": { "type": "array", "items": { "$ref": "#/definitions/Star" } },
                "count": { "type": "integer", "minimum": 0 }
            },
            "required": ["stars", "count"]
        },
        "definitions": { "Star": star_row() },
        "exit_codes": exit_codes()
    })
}
