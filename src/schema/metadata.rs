use serde_json::{json, Value};

use super::common::{count_array_response, exit_codes, SCHEMA_DRAFT, SESSION_REF_PATTERN};

fn note_row_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "integer", "minimum": 1 },
            "session_ref": {
                "type": "string",
                "pattern": SESSION_REF_PATTERN,
                "description": "<provider>/<session-id>[#<turn>]"
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
                    "reference": { "type": "string", "pattern": SESSION_REF_PATTERN },
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
                    "reference": { "type": "string", "pattern": SESSION_REF_PATTERN },
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

pub(super) fn note_schema() -> Value {
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
            "required": ["subcommand"]
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

fn tag_row_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "integer", "minimum": 1 },
            "session_ref": {
                "type": "string",
                "pattern": SESSION_REF_PATTERN,
                "description": "<provider>/<session-id>[#<turn>]"
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
                    "reference": { "type": "string", "pattern": SESSION_REF_PATTERN },
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
                    "reference": { "type": "string", "pattern": SESSION_REF_PATTERN },
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
                    "reference": { "type": "string", "pattern": SESSION_REF_PATTERN },
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

pub(super) fn tag_schema() -> Value {
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
            "required": ["subcommand"]
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

fn star_row() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_ref": {
                "type": "string",
                "pattern": SESSION_REF_PATTERN,
                "description": "<provider>/<session-id>[#<turn>]"
            },
            "starred_at": {
                "type": "string",
                "description": "ISO-8601 UTC, sub-second precision."
            }
        },
        "required": ["session_ref", "starred_at"]
    })
}

pub(super) fn star_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/star",
        "title": "aghist star",
        "command": "star",
        "description": "Mark a session or turn as starred. Stars live in the metadata sidecar (~/.local/share/aghist/metadata.db; AGHIST_METADATA_DB overrides). Starring an already-starred ref raises a `star-conflict` error. aghist never mutates provider history files.",
        "params": {
            "type": "object",
            "properties": {
                "reference": { "type": "string", "pattern": SESSION_REF_PATTERN }
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

pub(super) fn unstar_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/unstar",
        "title": "aghist unstar",
        "command": "unstar",
        "description": "Remove a star from a session or turn. Errors with `star-not-found` if the ref is not currently starred.",
        "params": {
            "type": "object",
            "properties": {
                "reference": { "type": "string", "pattern": SESSION_REF_PATTERN }
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

pub(super) fn stars_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/stars",
        "title": "aghist stars",
        "command": "stars",
        "description": "List starred sessions and turns. With no ref: every star, newest first. With `<provider>/<session-id>`: the session row plus any of its turns. With a turn-level ref: that turn exactly. Empty result exits 3.",
        "params": {
            "type": "object",
            "properties": {
                "reference": { "type": "string", "pattern": SESSION_REF_PATTERN },
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
