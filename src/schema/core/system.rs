use serde_json::{json, Value};

use crate::schema_fragments::health_response_schema;

use super::super::common::{
    closed_empty_object_schema, closed_object_schema, exit_codes, schema_props, SCHEMA_DRAFT,
};
use super::super::subcommands;

fn source_row_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("provider", json!({ "type": "string" })),
            (
                "paths",
                json!({ "type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "exists": { "type": "boolean" },
                        "bytes": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["path", "exists", "bytes"],
                    "additionalProperties": false
                } }),
            ),
            ("session_count", json!({ "type": "integer", "minimum": 0 })),
            ("total_bytes", json!({ "type": "integer", "minimum": 0 })),
            ("discover_error", json!({ "type": ["string", "null"] })),
        ]),
        &[
            "provider",
            "paths",
            "session_count",
            "total_bytes",
            "discover_error",
        ],
    )
}

fn source_name_schema() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "pattern": "^(?!local$)[A-Za-z0-9][A-Za-z0-9_-]*$",
        "description": "Remote source name. Must start with an ASCII letter or digit, may contain ASCII letters, digits, '-' and '_', and must not be `local`."
    })
}

fn remote_source_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("name", source_name_schema()),
            ("host", json!({ "type": "string" })),
            ("path", json!({ "type": "string" })),
            (
                "transport",
                json!({ "type": "string", "enum": ["ssh", "rsync"] }),
            ),
        ]),
        &["name", "host", "path", "transport"],
    )
}

fn remote_sources_payload_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "sources",
                json!({ "type": "array", "items": remote_source_schema() }),
            ),
            ("config_path", json!({ "type": "string" })),
        ]),
        &["sources", "config_path"],
    )
}

fn remote_source_mutation_response_schema(field: &'static str) -> Value {
    closed_object_schema(
        schema_props([
            (field, remote_source_schema()),
            ("config_path", json!({ "type": "string" })),
        ]),
        &[field, "config_path"],
    )
}

fn pull_result_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("name", json!({ "type": "string" })),
            ("host", json!({ "type": "string" })),
            ("path", json!({ "type": "string" })),
            (
                "transport",
                json!({ "type": "string", "enum": ["ssh", "rsync"] }),
            ),
            ("data_dir", json!({ "type": "string" })),
            ("dry_run", json!({ "type": "boolean" })),
            ("byte_count", json!({ "type": "integer", "minimum": 0 })),
            ("file_count", json!({ "type": "integer", "minimum": 0 })),
            (
                "pulled_at",
                json!({ "type": "string", "format": "date-time" }),
            ),
        ]),
        &[
            "name",
            "host",
            "path",
            "transport",
            "data_dir",
            "dry_run",
            "byte_count",
            "file_count",
            "pulled_at",
        ],
    )
}

fn pull_response_schema() -> Value {
    json!({
        "oneOf": [
            closed_object_schema(
                schema_props([
                    ("results", json!({ "type": "array", "items": pull_result_schema() })),
                    ("cache_dir", json!({ "type": "string" })),
                ]),
                &["results", "cache_dir"],
            ),
            {
                "description": "NDJSON output (one pull result per line).",
                "allOf": [pull_result_schema()]
            }
        ]
    })
}

fn machine_output_params() -> [(&'static str, Value); 2] {
    [
        ("json", json!({ "type": "boolean" })),
        ("ndjson", json!({ "type": "boolean" })),
    ]
}

fn sources_subcommands_schema() -> Value {
    json!({
        "add": {
            "description": "Register a new remote source and persist it to config.toml.",
            "params": closed_object_schema(
                schema_props([
                    ("name", source_name_schema()),
                    ("host", json!({ "type": "string" })),
                    ("path", json!({ "type": "string" })),
                    ("transport", json!({ "type": "string", "enum": ["ssh", "rsync"], "default": "ssh" })),
                    ("json", json!({ "type": "boolean" })),
                    ("ndjson", json!({ "type": "boolean" })),
                ]),
                &["name", "host", "path"],
            ),
            "response": remote_source_mutation_response_schema("added")
        },
        "list": {
            "description": "List registered remote sources.",
            "params": closed_object_schema(schema_props(machine_output_params()), &[]),
            "response": {
                "oneOf": [
                    remote_sources_payload_schema(),
                    {
                        "description": "NDJSON output (one remote source per line).",
                        "allOf": [remote_source_schema()]
                    }
                ]
            }
        },
        "remove": {
            "description": "Remove a registered remote source by name.",
            "params": closed_object_schema(
                schema_props([
                    ("name", source_name_schema()),
                    ("json", json!({ "type": "boolean" })),
                    ("ndjson", json!({ "type": "boolean" })),
                ]),
                &["name"],
            ),
            "response": remote_source_mutation_response_schema("removed")
        },
        "pull": {
            "description": "Pull one registered source, or all registered sources, into the local source cache.",
            "params": closed_object_schema(
                schema_props([
                    ("name", {
                        let mut schema = source_name_schema();
                        schema["description"] = json!("Source name. Mutually exclusive with all.");
                        schema
                    }),
                    ("all", json!({ "type": "boolean", "description": "Pull every registered source." })),
                    ("dry_run", json!({ "type": "boolean", "default": false })),
                    ("json", json!({ "type": "boolean" })),
                    ("ndjson", json!({ "type": "boolean" })),
                ]),
                &[],
            ),
            "response": pull_response_schema()
        }
    })
}

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
                    "allOf": [source_row_schema()]
                }
            ]
        },
        "subcommands": sources_subcommands_schema(),
        "definitions": {
            "SourceRow": source_row_schema(),
            "RemoteSource": remote_source_schema(),
            "PullResult": pull_result_schema()
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
                        "description": "Subcommand whose schema to emit. Use `--all` or `--list` on the CLI for index/dump."
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
