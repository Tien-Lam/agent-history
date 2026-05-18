use serde_json::{json, Value};

use crate::schema_fragments::{
    LIST_LIMIT_DEFAULT, SEARCH_HYBRID_WEIGHT_DEFAULT, SEARCH_LIMIT_DEFAULT,
    SEARCH_WATCH_INTERVAL_MS_DEFAULT, SEARCH_WATCH_ITERATIONS_DEFAULT,
    SHOW_INCLUDE_CONTEXT_DEFAULT,
};

use super::super::common::{
    closed_object_schema, exit_codes, filter_params_fragment, list_response_schema,
    provider_slug_enum, schema_props, search_response_schema, session_row_schema,
    source_qualified_citation_ref_pattern, source_qualified_session_only_ref_pattern,
    SchemaProperties, SCHEMA_DRAFT,
};

fn list_params_properties() -> SchemaProperties {
    let mut props = schema_props([
        (
            "json",
            json!({ "type": "boolean", "description": "Force JSON output (single object with `sessions` array)." }),
        ),
        (
            "ndjson",
            json!({ "type": "boolean", "description": "Force NDJSON output (one session per line)." }),
        ),
        (
            "limit",
            json!({ "type": "integer", "minimum": 1, "default": LIST_LIMIT_DEFAULT }),
        ),
        (
            "cursor",
            json!({ "type": "string", "description": "Opaque pagination cursor from a prior `meta.next_cursor`." }),
        ),
    ]);
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }
    props
}

fn search_params_properties() -> SchemaProperties {
    let mut props = schema_props([
        (
            "query",
            json!({ "type": "string", "description": "Tantivy query string. Mutually exclusive with query_file/stdin." }),
        ),
        (
            "query_file",
            json!({ "type": "string", "description": "Read query from file path (use '-' for stdin)." }),
        ),
        (
            "stdin",
            json!({ "type": "boolean", "description": "Read query from standard input." }),
        ),
        (
            "limit",
            json!({ "type": "integer", "minimum": 1, "default": SEARCH_LIMIT_DEFAULT }),
        ),
        (
            "cursor",
            json!({ "type": "string", "description": "Opaque pagination cursor from a prior `meta.next_cursor`." }),
        ),
        (
            "json",
            json!({ "type": "boolean", "description": "Force JSON output (default: JSON on pipe, table on TTY)." }),
        ),
        (
            "watch",
            json!({ "type": "boolean", "description": "Long-running NDJSON stream of new hits." }),
        ),
        (
            "watch_interval_ms",
            json!({ "type": "integer", "minimum": 1, "default": SEARCH_WATCH_INTERVAL_MS_DEFAULT }),
        ),
        (
            "watch_iterations",
            json!({ "type": "integer", "minimum": 0, "default": SEARCH_WATCH_ITERATIONS_DEFAULT, "description": "Stop after N polls (0 = run until interrupted)." }),
        ),
        (
            "hybrid_weight",
            json!({
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
                "default": SEARCH_HYBRID_WEIGHT_DEFAULT,
                "description": "RRF weight on the semantic side. 0.0 = lexical only (default), 1.0 = semantic only. Fails open to lexical when embeddings unavailable."
            }),
        ),
    ]);
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }
    props
}

pub(in crate::schema) fn list_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/list",
        "title": "aghist --list",
        "command": "--list",
        "description": "List sessions across enabled providers, sorted by start time descending.",
        "params": closed_object_schema(list_params_properties(), &[]),
        "response": {
            "oneOf": [
                list_response_schema(),
                {
                    "type": "object",
                    "description": "NDJSON output (one session per line) — each line matches this shape.",
                    "$ref": "#/definitions/SessionRow"
                }
            ]
        },
        "definitions": { "SessionRow": session_row_schema() },
        "exit_codes": exit_codes()
    })
}

pub(in crate::schema) fn search_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/search",
        "title": "aghist search",
        "command": "search",
        "description": "Full-text search across indexed sessions. Returns hits with citation refs.",
        "params": closed_object_schema(search_params_properties(), &[]),
        "response": search_response_schema(),
        "exit_codes": exit_codes()
    })
}

pub(in crate::schema) fn show_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/show",
        "title": "aghist show",
        "command": "show",
        "description": "Resolve a citation ref `<provider>/<session-id>#<turn>` or `<source>:<provider>/<session-id>#<turn>` to a single message.",
        "params": closed_object_schema(
            schema_props([
                (
                    "reference",
                    json!({
                        "type": "string",
                        "pattern": source_qualified_citation_ref_pattern(),
                        "description": "Citation ref. Examples: claude-code/abc-123#7, laptop:claude-code/abc-123#7"
                    }),
                ),
                (
                    "format",
                    json!({
                        "type": "string",
                        "enum": ["md", "json", "text"],
                        "default": "md"
                    }),
                ),
                (
                    "include_context",
                    json!({
                        "type": "integer",
                        "minimum": 0,
                        "default": SHOW_INCLUDE_CONTEXT_DEFAULT,
                        "description": "Number of turns before and after the target to include."
                    }),
                ),
            ]),
            &["reference"],
        ),
        "response": {
            "type": "object",
            "description": "JSON output (when --format=json). Other formats emit text/markdown.",
            "properties": {
                "ref": { "type": "string" },
                "provider": { "type": "string", "enum": provider_slug_enum() },
                "session_id": { "type": "string" },
                "project": { "type": ["string", "null"] },
                "target_turn": { "type": "integer", "minimum": 1 },
                "messages": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "turn": { "type": "integer", "minimum": 1 },
                            "is_target": { "type": "boolean" },
                            "role": { "type": "string" },
                            "content": { "type": "array" },
                            "timestamp": { "type": "string", "format": "date-time" }
                        },
                        "required": ["turn", "is_target", "role", "content", "timestamp"]
                    }
                }
            },
            "required": ["ref", "provider", "session_id", "target_turn", "messages"]
        },
        "exit_codes": exit_codes()
    })
}

pub(in crate::schema) fn export_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/export",
        "title": "aghist export",
        "command": "export",
        "description": "Export a session to Markdown, JSON, or HTML.",
        "params": closed_object_schema(
            schema_props([
                ("format", json!({ "type": "string", "enum": ["md", "json", "html"] })),
                (
                    "session",
                    json!({ "type": "string", "description": "Session ID/prefix, `<provider>/<session-id>`, or `<source>:<provider>/<session-id>`." }),
                ),
                (
                    "output",
                    json!({ "type": "string", "description": "Output file path (defaults to stdout)." }),
                ),
                (
                    "turn_range",
                    json!({
                        "type": "string",
                        "pattern": "^[0-9]*(:[0-9]*)?$",
                        "description": "1-based inclusive turn range: A:B, :B, A:, or a single A."
                    }),
                ),
                (
                    "include_notes",
                    json!({
                        "type": "boolean",
                        "description": "Inline private annotations (from the metadata sidecar) at their citation refs. Notes stay marked 'private annotation' in the output."
                    }),
                ),
            ]),
            &["format", "session"],
        ),
        "response": {
            "description": "Raw exported content written to stdout or `output` path. Format depends on `format` param."
        },
        "exit_codes": exit_codes()
    })
}

pub(in crate::schema) fn diff_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/diff",
        "title": "aghist diff",
        "command": "diff",
        "description": "Compare two sessions turn-by-turn using longest-common-subsequence over role + content snippets.",
        "params": closed_object_schema(
            schema_props([
                (
                    "session1",
                    json!({
                        "type": "string",
                        "pattern": source_qualified_session_only_ref_pattern(),
                        "description": "First session ref. Examples: claude-code/abc-123, laptop:claude-code/abc-123"
                    }),
                ),
                (
                    "session2",
                    json!({
                        "type": "string",
                        "pattern": source_qualified_session_only_ref_pattern(),
                        "description": "Second session ref. Examples: claude-code/def-456, laptop:claude-code/def-456"
                    }),
                ),
                (
                    "context",
                    json!({
                        "type": "integer",
                        "minimum": 0,
                        "default": 2,
                        "description": "Context lines around each changed hunk in text output."
                    }),
                ),
                (
                    "json",
                    json!({
                        "type": "boolean",
                        "description": "Force JSON output."
                    }),
                ),
            ]),
            &["session1", "session2"],
        ),
        "response": {
            "type": "object",
            "description": "JSON output when --json or stdout is not a TTY. Text output is unified-diff style.",
            "properties": {
                "session1": { "$ref": "#/definitions/DiffSession" },
                "session2": { "$ref": "#/definitions/DiffSession" },
                "ops": {
                    "type": "array",
                    "items": { "$ref": "#/definitions/DiffOp" }
                },
                "changed": { "type": "integer", "minimum": 0 },
                "same": { "type": "integer", "minimum": 0 }
            },
            "required": ["session1", "session2", "ops", "changed", "same"]
        },
        "definitions": {
            "DiffSession": {
                "type": "object",
                "properties": {
                    "ref": { "type": "string", "pattern": source_qualified_session_only_ref_pattern() },
                    "started_at": { "type": "string", "format": "date-time" },
                    "turns": { "type": "integer", "minimum": 0 }
                },
                "required": ["ref", "started_at", "turns"]
            },
            "DiffOp": {
                "type": "object",
                "properties": {
                    "op": { "type": "string", "enum": ["same", "delete", "insert"] },
                    "role": { "type": "string" },
                    "snippet": { "type": "string" },
                    "turn_a": { "type": "integer", "minimum": 1 },
                    "turn_b": { "type": "integer", "minimum": 1 }
                },
                "required": ["op", "role", "snippet"]
            }
        },
        "exit_codes": exit_codes()
    })
}
