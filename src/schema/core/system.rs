use serde_json::{json, Value};

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
        "response": {
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" },
                "checks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "status": { "type": "string", "enum": ["ok", "warn", "fail"] },
                            "message": { "type": "string" },
                            "hint": { "type": ["string", "null"] }
                        },
                        "required": ["name", "status", "message"]
                    }
                },
                "summary": {
                    "type": "object",
                    "properties": {
                        "ok_count": { "type": "integer", "minimum": 0 },
                        "warn_count": { "type": "integer", "minimum": 0 },
                        "fail_count": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["ok_count", "warn_count", "fail_count"]
                },
                "provider_fidelity": {
                    "type": "array",
                    "items": provider_fidelity_item_schema()
                },
            },
            "required": ["ok", "checks", "summary", "provider_fidelity"]
        },
        "exit_codes": exit_codes()
    })
}

fn provider_fidelity_item_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "label": { "type": "string" },
            "provider": { "type": "string" },
            "session_count": { "type": "integer", "minimum": 0 },
            "message_count": { "type": "integer", "minimum": 0 },
            "parse": {
                "type": "object",
                "properties": {
                    "records_seen": { "type": "integer", "minimum": 0 },
                    "parse_errors": { "type": "integer", "minimum": 0 },
                    "skipped_records": { "type": "integer", "minimum": 0 },
                    "empty_content": { "type": "integer", "minimum": 0 },
                },
                "required": ["records_seen", "parse_errors", "skipped_records", "empty_content"]
            },
            "blocks": {
                "type": "object",
                "properties": {
                    "text": { "type": "integer", "minimum": 0 },
                    "code_block": { "type": "integer", "minimum": 0 },
                    "tool_use": { "type": "integer", "minimum": 0 },
                    "tool_result": { "type": "integer", "minimum": 0 },
                    "thinking": { "type": "integer", "minimum": 0 },
                    "error": { "type": "integer", "minimum": 0 },
                    "total": { "type": "integer", "minimum": 0 },
                },
                "required": ["text", "code_block", "tool_use", "tool_result", "thinking", "error", "total"]
            },
            "tool_call_fidelity": {
                "type": "object",
                "properties": {
                    "tool_calls": { "type": "integer", "minimum": 0 },
                    "tool_results": { "type": "integer", "minimum": 0 },
                    "paired": { "type": "integer", "minimum": 0 },
                    "unpaired_calls": { "type": "integer", "minimum": 0 },
                    "orphan_results": { "type": "integer", "minimum": 0 },
                    "empty_names": { "type": "integer", "minimum": 0 },
                    "empty_call_ids": { "type": "integer", "minimum": 0 },
                    "empty_result_ids": { "type": "integer", "minimum": 0 },
                    "invalid_json_args": { "type": "integer", "minimum": 0 },
                    "success_results": { "type": "integer", "minimum": 0 },
                    "failure_results": { "type": "integer", "minimum": 0 },
                },
                "required": [
                    "tool_calls",
                    "tool_results",
                    "paired",
                    "unpaired_calls",
                    "orphan_results",
                    "empty_names",
                    "empty_call_ids",
                    "empty_result_ids",
                    "invalid_json_args",
                    "success_results",
                    "failure_results"
                ]
            },
        },
        "required": [
            "label",
            "provider",
            "session_count",
            "message_count",
            "parse",
            "blocks",
            "tool_call_fidelity"
        ]
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
