use serde_json::{json, Value};

use super::common::{
    exit_codes, filter_params_fragment, provider_slug_enum, provider_slug_enum_nullable,
    session_row_schema, SCHEMA_DRAFT,
};
use super::subcommands;

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

pub(super) fn list_schema() -> Value {
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

pub(super) fn search_schema() -> Value {
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

pub(super) fn show_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/show",
        "title": "aghist show",
        "command": "show",
        "description": "Resolve a citation ref `<provider>/<session-id>#<turn>` to a single message.",
        "params": {
            "type": "object",
            "properties": {
                "reference": {
                    "type": "string",
                    "pattern": "^[a-z0-9-]+/.+#[1-9][0-9]*$",
                    "description": "Citation ref. Example: claude-code/abc-123#7"
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

pub(super) fn export_schema() -> Value {
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

pub(super) fn index_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/index",
        "title": "aghist index",
        "command": "index",
        "description": "Build or refresh the search index. Idempotent and delta-aware.",
        "params": {
            "type": "object",
            "properties": {
                "provider": { "type": "string", "enum": provider_slug_enum() },
                "force": { "type": "boolean", "default": false, "description": "Clear the index before rebuilding." },
                "accept_download": { "type": "boolean", "default": false, "description": "Authorise the embedding-model download (~90 MB)." }
            },
            "additionalProperties": false
        },
        "response": {
            "type": "object",
            "properties": {
                "providers": { "type": "array", "items": { "type": "string", "enum": provider_slug_enum() } },
                "sessions_total": { "type": "integer", "minimum": 0 },
                "added": { "type": "integer", "minimum": 0 },
                "updated": { "type": "integer", "minimum": 0 },
                "unchanged": { "type": "integer", "minimum": 0 },
                "messages_indexed": { "type": "integer", "minimum": 0 },
                "force": { "type": "boolean" },
                "index_dir": { "type": "string" },
                "duration_ms": { "type": "integer", "minimum": 0 },
                "errors": { "type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "provider": { "type": "string", "enum": provider_slug_enum() },
                        "error": { "type": "string" }
                    },
                    "required": ["provider", "error"]
                } },
                "embeddings": {
                    "type": "object",
                    "description": "Status of the optional semantic-embedding pass. Shape varies by status.",
                    "properties": {
                        "status": { "type": "string", "enum": ["disabled", "awaiting-consent", "enabled"] }
                    },
                    "required": ["status"]
                }
            },
            "required": ["providers", "sessions_total", "added", "updated", "unchanged", "messages_indexed", "force", "index_dir", "duration_ms", "errors", "embeddings"]
        },
        "exit_codes": exit_codes()
    })
}

pub(super) fn sources_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/sources",
        "title": "aghist sources",
        "command": "sources",
        "description": "List detected provider sources: paths, session counts, sizes, last-indexed-at.",
        "params": {
            "type": "object",
            "properties": {
                "json": { "type": "boolean" },
                "ndjson": { "type": "boolean" }
            },
            "additionalProperties": false
        },
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

pub(super) fn health_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/health",
        "title": "aghist health",
        "command": "health",
        "description": "Machine-readable doctor: validates index, manifest, and provider state.",
        "params": {
            "type": "object",
            "properties": {
                "json": { "type": "boolean" },
                "ndjson": { "type": "boolean" }
            },
            "additionalProperties": false
        },
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
                }
            },
            "required": ["ok", "checks", "summary"]
        },
        "exit_codes": exit_codes()
    })
}

pub(super) fn mcp_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/mcp",
        "title": "aghist mcp",
        "command": "mcp",
        "description": "Run a stdio MCP server (JSON-RPC 2.0, newline-delimited) exposing aghist's read paths.",
        "params": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        },
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

pub(super) fn schema_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/schema",
        "title": "aghist schema",
        "command": "schema",
        "description": "Emit JSON-Schema (draft-2020-12) describing aghist subcommands.",
        "params": {
            "type": "object",
            "properties": {
                "subcommand": {
                    "type": "string",
                    "enum": subcommands(),
                    "description": "Subcommand whose schema to emit. Use 'all' or '--list' on the CLI for index/dump."
                },
                "list": { "type": "boolean", "description": "List available schema subcommand names." },
                "all": { "type": "boolean", "description": "Emit every schema as a single object keyed by subcommand." }
            },
            "additionalProperties": false
        },
        "response": {
            "description": "JSON-Schema document for a single subcommand, OR `{ subcommands: [...] }` with --list, OR a map of name → schema with --all."
        },
        "exit_codes": exit_codes()
    })
}
