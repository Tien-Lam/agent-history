use serde_json::{json, Value};

use crate::schema_fragments::{
    LIST_LIMIT_DEFAULT, SEARCH_HYBRID_WEIGHT_DEFAULT, SEARCH_LIMIT_DEFAULT,
    SEARCH_WATCH_INTERVAL_MS_DEFAULT, SEARCH_WATCH_ITERATIONS_DEFAULT,
    SHOW_INCLUDE_CONTEXT_DEFAULT,
};

use super::super::common::{
    exit_codes, filter_params_fragment, provider_slug_enum, provider_slug_enum_nullable,
    session_row_schema, source_qualified_citation_ref_pattern,
    source_qualified_session_only_ref_pattern, source_qualified_session_ref_pattern, SCHEMA_DRAFT,
};

fn list_params_properties() -> Value {
    let mut props = serde_json::Map::new();
    props.insert(
        "json".to_string(),
        json!({ "type": "boolean", "description": "Force JSON output (single object with `sessions` array)." }),
    );
    props.insert(
        "ndjson".to_string(),
        json!({ "type": "boolean", "description": "Force NDJSON output (one session per line)." }),
    );
    props.insert(
        "limit".to_string(),
        json!({ "type": "integer", "minimum": 1, "default": LIST_LIMIT_DEFAULT }),
    );
    props.insert(
        "cursor".to_string(),
        json!({ "type": "string", "description": "Opaque pagination cursor from a prior `meta.next_cursor`." }),
    );
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }
    Value::Object(props)
}

fn search_params_properties() -> Value {
    let mut props = serde_json::Map::new();
    props.insert(
        "query".to_string(),
        json!({ "type": "string", "description": "Tantivy query string. Mutually exclusive with query_file/stdin." }),
    );
    props.insert(
        "query_file".to_string(),
        json!({ "type": "string", "description": "Read query from file path (use '-' for stdin)." }),
    );
    props.insert(
        "stdin".to_string(),
        json!({ "type": "boolean", "description": "Read query from standard input." }),
    );
    props.insert(
        "limit".to_string(),
        json!({ "type": "integer", "minimum": 1, "default": SEARCH_LIMIT_DEFAULT }),
    );
    props.insert(
        "cursor".to_string(),
        json!({ "type": "string", "description": "Opaque pagination cursor from a prior `meta.next_cursor`." }),
    );
    props.insert(
        "json".to_string(),
        json!({ "type": "boolean", "description": "Force JSON output (default: JSON on pipe, table on TTY)." }),
    );
    props.insert(
        "watch".to_string(),
        json!({ "type": "boolean", "description": "Long-running NDJSON stream of new hits." }),
    );
    props.insert(
        "watch_interval_ms".to_string(),
        json!({ "type": "integer", "minimum": 1, "default": SEARCH_WATCH_INTERVAL_MS_DEFAULT }),
    );
    props.insert(
        "watch_iterations".to_string(),
        json!({ "type": "integer", "minimum": 0, "default": SEARCH_WATCH_ITERATIONS_DEFAULT, "description": "Stop after N polls (0 = run until interrupted)." }),
    );
    props.insert(
        "hybrid_weight".to_string(),
        json!({
            "type": "number",
            "minimum": 0.0,
            "maximum": 1.0,
            "default": SEARCH_HYBRID_WEIGHT_DEFAULT,
            "description": "RRF weight on the semantic side. 0.0 = lexical only (default), 1.0 = semantic only. Fails open to lexical when embeddings unavailable."
        }),
    );
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }
    Value::Object(props)
}

pub(in crate::schema) fn list_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/list",
        "title": "aghist --list",
        "command": "--list",
        "description": "List sessions across enabled providers, sorted by start time descending.",
        "params": {
            "type": "object",
            "properties": list_params_properties(),
            "additionalProperties": false
        },
        "response": {
            "oneOf": [
                {
                    "type": "object",
                    "description": "JSON output (when --json or stdout is not a TTY).",
                    "properties": {
                        "sessions": { "type": "array", "items": session_row_schema() }
                    },
                    "required": ["sessions"]
                },
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
        "params": {
            "type": "object",
            "properties": search_params_properties(),
            "additionalProperties": false
        },
        "response": search_response_schema(),
        "exit_codes": exit_codes()
    })
}

fn search_response_schema() -> Value {
    json!({
        "type": "object",
        "description": "JSON envelope emitted by `aghist search --json`; watch mode emits one hit object per NDJSON line.",
        "properties": {
            "hits": {
                "type": "array",
                "description": "Hits ordered by score descending then started_at descending.",
                "items": search_hit_schema()
            },
            "meta": {
                "type": "object",
                "properties": {
                    "next_cursor": {
                        "type": ["string", "null"],
                        "description": "Opaque pagination cursor; pass back with --cursor."
                    },
                    "total": { "type": "integer", "minimum": 0 },
                    "engine": {
                        "type": "string",
                        "enum": ["lexical", "hybrid"],
                        "description": "Search engine that produced the results."
                    }
                },
                "required": ["next_cursor", "total", "engine"],
                "additionalProperties": false
            }
        },
        "required": ["hits", "meta"],
        "additionalProperties": false
    })
}

fn search_hit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["message", "note"],
                "description": "Whether the hit points at a session message or a metadata note."
            },
            "session_id": { "type": "string" },
            "message_id": { "type": "string" },
            "score": { "type": "number" },
            "snippet": { "type": "string" },
            "provider": { "type": ["string", "null"], "enum": provider_slug_enum_nullable() },
            "project": { "type": ["string", "null"] },
            "started_at": { "type": ["string", "null"], "format": "date-time" },
            "source": {
                "type": "string",
                "description": "`local` for this host, or a registered remote source name."
            },
            "note_id": {
                "type": "integer",
                "description": "Present for note hits; absent for message hits."
            },
            "ref": {
                "type": "string",
                "pattern": source_qualified_session_ref_pattern(),
                "description": "Citation ref for message hits, or the note's stored session ref for note hits."
            },
            "explanation": {
                "type": "object",
                "description": "Present only with --debug-search; Tantivy score explanation tree."
            }
        },
        "required": [
            "kind",
            "session_id",
            "message_id",
            "score",
            "snippet",
            "provider",
            "project",
            "started_at",
            "source"
        ],
        "additionalProperties": false
    })
}

pub(in crate::schema) fn show_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/show",
        "title": "aghist show",
        "command": "show",
        "description": "Resolve a citation ref `<provider>/<session-id>#<turn>` or `<source>:<provider>/<session-id>#<turn>` to a single message.",
        "params": {
            "type": "object",
            "properties": {
                "reference": {
                    "type": "string",
                    "pattern": source_qualified_citation_ref_pattern(),
                    "description": "Citation ref. Examples: claude-code/abc-123#7, laptop:claude-code/abc-123#7"
                },
                "format": {
                    "type": "string",
                    "enum": ["md", "json", "text"],
                    "default": "md"
                },
                "include_context": {
                    "type": "integer",
                    "minimum": 0,
                    "default": SHOW_INCLUDE_CONTEXT_DEFAULT,
                    "description": "Number of turns before and after the target to include."
                }
            },
            "required": ["reference"],
            "additionalProperties": false
        },
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
        "params": {
            "type": "object",
            "properties": {
                "format": { "type": "string", "enum": ["md", "json", "html"] },
                "session": { "type": "string", "description": "Session ID/prefix, `<provider>/<session-id>`, or `<source>:<provider>/<session-id>`." },
                "output": { "type": "string", "description": "Output file path (defaults to stdout)." },
                "turn_range": {
                    "type": "string",
                    "pattern": "^[0-9]*(:[0-9]*)?$",
                    "description": "1-based inclusive turn range: A:B, :B, A:, or a single A."
                },
                "include_notes": {
                    "type": "boolean",
                    "description": "Inline private annotations (from the metadata sidecar) at their citation refs. Notes stay marked 'private annotation' in the output."
                }
            },
            "required": ["format", "session"],
            "additionalProperties": false
        },
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
        "params": {
            "type": "object",
            "properties": {
                "session1": {
                    "type": "string",
                    "pattern": source_qualified_session_only_ref_pattern(),
                    "description": "First session ref. Examples: claude-code/abc-123, laptop:claude-code/abc-123"
                },
                "session2": {
                    "type": "string",
                    "pattern": source_qualified_session_only_ref_pattern(),
                    "description": "Second session ref. Examples: claude-code/def-456, laptop:claude-code/def-456"
                },
                "context": {
                    "type": "integer",
                    "minimum": 0,
                    "default": 2,
                    "description": "Context lines around each changed hunk in text output."
                },
                "json": {
                    "type": "boolean",
                    "description": "Force JSON output."
                }
            },
            "required": ["session1", "session2"],
            "additionalProperties": false
        },
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
