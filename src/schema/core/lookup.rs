use serde_json::{json, Value};

use super::super::common::{
    exit_codes, filter_params_fragment, provider_slug_enum, provider_slug_enum_nullable,
    session_row_schema, SCHEMA_DRAFT, SOURCE_QUALIFIED_CITATION_REF_PATTERN,
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
        json!({ "type": "integer", "minimum": 1, "default": 20 }),
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
        json!({ "type": "integer", "minimum": 1, "default": 2000 }),
    );
    props.insert(
        "watch_iterations".to_string(),
        json!({ "type": "integer", "minimum": 0, "default": 0, "description": "Stop after N polls (0 = run until interrupted)." }),
    );
    props.insert(
        "hybrid_weight".to_string(),
        json!({
            "type": "number",
            "minimum": 0.0,
            "maximum": 1.0,
            "default": 0.0,
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
        "response": {
            "type": "array",
            "description": "Array of hits, ordered by score descending then started_at descending.",
            "items": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "message_id": { "type": "string" },
                    "score": { "type": "number" },
                    "snippet": { "type": "string" },
                    "provider": { "type": ["string", "null"], "enum": provider_slug_enum_nullable() },
                    "project": { "type": ["string", "null"] },
                    "started_at": { "type": ["string", "null"], "format": "date-time" }
                },
                "required": ["session_id", "message_id", "score", "snippet"]
            }
        },
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
        "params": {
            "type": "object",
            "properties": {
                "reference": {
                    "type": "string",
                    "pattern": SOURCE_QUALIFIED_CITATION_REF_PATTERN,
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
                    "default": 0,
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
                "session": { "type": "string", "description": "Session ID or unique prefix." },
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
