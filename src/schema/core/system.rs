use serde_json::{json, Value};

use crate::schema_fragments::health_response_schema;

use super::super::common::{
    closed_empty_object_schema, closed_object_schema, exit_codes, schema_props, SCHEMA_DRAFT,
};
use super::super::subcommands;

pub(in crate::schema) fn sources_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/sources",
        "title": "aghist sources",
        "command": "sources",
        "description": "List detected provider sources: paths, session counts, sizes, last-indexed-at.",
        "params": closed_object_schema(
            schema_props([
                ("json", json!({ "type": "boolean" })),
                ("ndjson", json!({ "type": "boolean" })),
            ]),
            &[],
        ),
        "response": {
            "oneOf": [
                {
                    "type": "object",
                    "description": "JSON output.",
                    "properties": {
                        "sources": { "type": "array", "items": { "$ref": "#/definitions/SourceRow" } },
                        "index": {
                            "type": "object",
                            "properties": {
                                "dir": { "type": "string" },
                                "last_indexed_at": { "type": ["string", "null"], "format": "date-time" }
                            },
                            "required": ["dir"]
                        }
                    },
                    "required": ["sources", "index"]
                },
                {
                    "description": "NDJSON output (one source row per line).",
                    "$ref": "#/definitions/SourceRow"
                }
            ]
        },
        "definitions": {
            "SourceRow": {
                "type": "object",
                "properties": {
                    "provider": { "type": "string" },
                    "paths": { "type": "array", "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "exists": { "type": "boolean" },
                            "bytes": { "type": "integer", "minimum": 0 }
                        },
                        "required": ["path", "exists", "bytes"]
                    } },
                    "session_count": { "type": "integer", "minimum": 0 },
                    "total_bytes": { "type": "integer", "minimum": 0 },
                    "discover_error": { "type": ["string", "null"] }
                },
                "required": ["provider", "paths", "session_count", "total_bytes"]
            }
        },
        "exit_codes": exit_codes()
    })
}

pub(in crate::schema) fn health_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/health",
        "title": "aghist health",
        "command": "health",
        "description": "Machine-readable doctor: validates index, manifest, and provider state.",
        "params": closed_object_schema(
            schema_props([
                ("json", json!({ "type": "boolean" })),
                ("ndjson", json!({ "type": "boolean" })),
            ]),
            &[],
        ),
        "response": health_response_schema(),
        "exit_codes": exit_codes()
    })
}

pub(in crate::schema) fn mcp_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/mcp",
        "title": "aghist mcp",
        "command": "mcp",
        "description": "Run a stdio MCP server (JSON-RPC 2.0, newline-delimited) exposing aghist's read paths.",
        "params": closed_empty_object_schema(),
        "response": {
            "description": "Newline-delimited JSON-RPC 2.0 messages on stdin/stdout. Tools: search_sessions, list_sessions, get_session, get_message, reindex, health."
        },
        "exit_codes": {
            "0": "server exited cleanly (stdin closed)",
            "1": "runtime error (envelope on stderr)",
            "2": "usage error"
        }
    })
}

pub(in crate::schema) fn schema_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/schema",
        "title": "aghist schema",
        "command": "schema",
        "description": "Emit JSON-Schema (draft-2020-12) describing aghist subcommands.",
        "params": closed_object_schema(
            schema_props([
                (
                    "subcommand",
                    json!({
                        "type": "string",
                        "enum": subcommands(),
                        "description": "Subcommand whose schema to emit. Use 'all' or '--list' on the CLI for index/dump."
                    }),
                ),
                (
                    "list",
                    json!({ "type": "boolean", "description": "List available schema subcommand names." }),
                ),
                (
                    "all",
                    json!({ "type": "boolean", "description": "Emit every schema as a single object keyed by subcommand." }),
                ),
            ]),
            &[],
        ),
        "response": {
            "description": "JSON-Schema document for a single subcommand, OR `{ subcommands: [...] }` with --list, OR a map of name → schema with --all."
        },
        "exit_codes": exit_codes()
    })
}
