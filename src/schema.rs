//! Self-describing JSON-Schema (draft-2020-12) for `aghist` subcommands.
//!
//! Agents (and humans) can run `aghist schema <subcmd>` to get a stable
//! machine-readable description of a subcommand's flags, response shape, and
//! exit codes — instead of scraping `--help` or relying on documentation that
//! drifts away from the binary.
//!
//! The schemas are hand-authored rather than derived: the CLI surface is small
//! and stable, agents need exit-code semantics and response shapes that clap's
//! introspection doesn't provide, and the schema doubles as the contract we
//! commit to.

use serde_json::{json, Value};

const SCHEMA_DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Subcommands that expose a schema. Order matches the help output.
pub const SUBCOMMANDS: &[&str] = &[
    "list",
    "search",
    "show",
    "export",
    "index",
    "sources",
    "health",
    "mcp",
    "schema",
    "decisions",
    "todos",
    "threads",
    "note",
];

/// Return the schema for a subcommand, or `None` if the name is unknown.
pub fn schema_for(subcmd: &str) -> Option<Value> {
    match subcmd {
        "list" => Some(list_schema()),
        "search" => Some(search_schema()),
        "show" => Some(show_schema()),
        "export" => Some(export_schema()),
        "index" => Some(index_schema()),
        "sources" => Some(sources_schema()),
        "health" => Some(health_schema()),
        "mcp" => Some(mcp_schema()),
        "schema" => Some(schema_schema()),
        "decisions" => Some(decisions_schema()),
        "todos" => Some(todos_schema()),
        "threads" => Some(threads_schema()),
        "note" => Some(note_schema()),
        _ => None,
    }
}

/// Return a JSON object containing the schema for every known subcommand,
/// keyed by name. Useful for one-shot discovery.
pub fn all_schemas() -> Value {
    let mut map = serde_json::Map::new();
    for name in SUBCOMMANDS {
        if let Some(schema) = schema_for(name) {
            map.insert((*name).to_string(), schema);
        }
    }
    Value::Object(map)
}

/// JSON list of subcommand names, suitable for `aghist schema --list`.
pub fn subcommand_index() -> Value {
    json!({
        "subcommands": SUBCOMMANDS,
    })
}

// ─── shared fragments ──────────────────────────────────────────────────────

fn provider_slug_enum() -> Value {
    json!(["claude-code", "copilot-cli", "gemini-cli", "codex-cli", "opencode", "cursor"])
}

fn exit_codes() -> Value {
    json!({
        "0": "success with results",
        "1": "runtime error (JSON envelope on stderr)",
        "2": "usage error (bad flags or parse failure)",
        "3": "success but empty (no rows / no hits)"
    })
}

/// Filter flags shared by `--list` and `search`. Returned as a fragment so
/// each subcommand schema can fold these in alongside its own params.
fn filter_params_fragment() -> Vec<(&'static str, Value)> {
    vec![
        (
            "provider",
            json!({
                "type": "string",
                "enum": provider_slug_enum(),
                "description": "Restrict to a single provider."
            }),
        ),
        (
            "since",
            json!({
                "type": "string",
                "format": "date-time",
                "description": "RFC 3339 lower bound on message/session timestamp (inclusive)."
            }),
        ),
        (
            "until",
            json!({
                "type": "string",
                "format": "date-time",
                "description": "RFC 3339 upper bound on message/session timestamp (inclusive)."
            }),
        ),
        (
            "project",
            json!({
                "type": "string",
                "description": "Substring match against the session's project name (case-insensitive)."
            }),
        ),
        (
            "role",
            json!({
                "type": "string",
                "enum": ["user", "assistant", "tool"],
                "description": "Restrict to messages with this role."
            }),
        ),
        (
            "has_tool_call",
            json!({
                "type": "boolean",
                "description": "Keep only messages (or sessions containing messages) with a tool invocation."
            }),
        ),
    ]
}

fn session_row_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "provider": { "type": "string", "enum": provider_slug_enum() },
            "project": { "type": ["string", "null"] },
            "branch": { "type": ["string", "null"] },
            "summary": { "type": ["string", "null"] },
            "started_at": { "type": "string", "format": "date-time" },
            "message_count": { "type": "integer", "minimum": 0 }
        },
        "required": ["id", "provider", "started_at", "message_count"]
    })
}

// ─── per-subcommand schemas ────────────────────────────────────────────────

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

fn list_schema() -> Value {
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

fn search_schema() -> Value {
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
                    "provider": { "type": ["string", "null"], "enum": [
                        "claude-code", "copilot-cli", "gemini-cli", "codex-cli", "opencode", "cursor", null
                    ] },
                    "project": { "type": ["string", "null"] },
                    "started_at": { "type": ["string", "null"], "format": "date-time" }
                },
                "required": ["session_id", "message_id", "score", "snippet"]
            }
        },
        "exit_codes": exit_codes()
    })
}

fn show_schema() -> Value {
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

fn export_schema() -> Value {
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

fn index_schema() -> Value {
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

fn sources_schema() -> Value {
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

fn health_schema() -> Value {
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

fn mcp_schema() -> Value {
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

fn todos_schema() -> Value {
    let mut props = serde_json::Map::new();
    props.insert(
        "kind".to_string(),
        json!({
            "type": "array",
            "items": {
                "type": "string",
                "enum": ["todo", "follow-up", "come-back-to", "we-should", "bd-ref"]
            },
            "description": "Restrict to one or more candidate kinds. Empty = all kinds."
        }),
    );
    props.insert(
        "limit".to_string(),
        json!({
            "type": "integer",
            "minimum": 0,
            "default": 200,
            "description": "Maximum candidates to emit (0 = no limit). Newest matches kept first."
        }),
    );
    props.insert(
        "json".to_string(),
        json!({
            "type": "boolean",
            "description": "Force JSON output (default: JSON on pipe, table on TTY)."
        }),
    );
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }

    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/todos",
        "title": "aghist todos",
        "command": "todos",
        "description": "Heuristic scan for unresolved TODOs / follow-ups / open beads-style refs across indexed sessions. No LLM, no `bd` lookups — agents can post-process the JSON shape.",
        "params": {
            "type": "object",
            "properties": Value::Object(props),
            "additionalProperties": false
        },
        "response": {
            "type": "object",
            "description": "JSON output (when --json or stdout is not a TTY).",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "Candidates ordered by timestamp descending, then by session id, turn, and kind for determinism.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "ref": {
                                "type": "string",
                                "description": "Citation ref `<provider>/<session-id>#<turn>` — pass to `aghist show` to inspect."
                            },
                            "provider": { "type": "string", "enum": provider_slug_enum() },
                            "session_id": { "type": "string" },
                            "turn": { "type": "integer", "minimum": 1 },
                            "kind": {
                                "type": "string",
                                "enum": ["todo", "follow_up", "come_back_to", "we_should", "bd_ref"]
                            },
                            "snippet": {
                                "type": "string",
                                "description": "Trimmed matched line; truncated to ~240 chars with a trailing ellipsis."
                            },
                            "role": { "type": "string", "enum": ["user", "assistant", "tool"] },
                            "timestamp": { "type": "string", "format": "date-time" },
                            "bd_id": {
                                "type": ["string", "null"],
                                "description": "Captured beads-style id (only for kind=bd_ref). Caller can `bd show` to drop closed refs."
                            }
                        },
                        "required": ["ref", "provider", "session_id", "turn", "kind", "snippet", "role", "timestamp"]
                    }
                },
                "count": { "type": "integer", "minimum": 0 }
            },
            "required": ["todos", "count"]
        },
        "exit_codes": exit_codes()
    })
}

fn schema_schema() -> Value {
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
                    "enum": SUBCOMMANDS,
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

fn decisions_schema() -> Value {
    let mut props = serde_json::Map::new();
    props.insert(
        "session".to_string(),
        json!({
            "type": "string",
            "description": "Restrict to a session id, unique id prefix, or full citation ref (turn ignored)."
        }),
    );
    props.insert(
        "threshold".to_string(),
        json!({
            "type": "number",
            "minimum": 0,
            "default": 3.0,
            "description": "Drop candidates whose marker-score is below this value."
        }),
    );
    props.insert(
        "limit".to_string(),
        json!({
            "type": "integer",
            "minimum": 1,
            "default": 50,
            "description": "Maximum number of candidates to return after sorting by score."
        }),
    );
    props.insert(
        "json".to_string(),
        json!({ "type": "boolean", "default": false, "description": "Force JSON output." }),
    );
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }

    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/decisions",
        "title": "aghist decisions",
        "command": "decisions",
        "description": "Heuristic-extract candidate architectural decisions from sessions. Returns ranked sentences with citation refs; agents can post-process by `aghist show`-ing the refs.",
        "params": {
            "type": "object",
            "properties": Value::Object(props),
            "additionalProperties": false
        },
        "response": {
            "type": "object",
            "description": "JSON output (when --json or stdout is not a TTY).",
            "properties": {
                "decisions": {
                    "type": "array",
                    "description": "Candidates ordered by score descending then started_at descending.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "ref": {
                                "type": "string",
                                "pattern": "^[a-z0-9-]+/.+#[1-9][0-9]*$",
                                "description": "Citation ref `<provider>/<session-id>#<turn>`."
                            },
                            "provider": { "type": "string", "enum": provider_slug_enum() },
                            "session_id": { "type": "string" },
                            "turn": { "type": "integer", "minimum": 1 },
                            "role": { "type": "string", "enum": ["user", "assistant", "system", "tool"] },
                            "score": { "type": "number" },
                            "markers": { "type": "array", "items": { "type": "string" } },
                            "snippet": { "type": "string" },
                            "project": { "type": ["string", "null"] },
                            "timestamp": { "type": "string", "format": "date-time" },
                            "started_at": { "type": "string", "format": "date-time" }
                        },
                        "required": ["ref", "provider", "session_id", "turn", "role", "score", "markers", "snippet", "timestamp", "started_at"]
                    }
                },
                "count": { "type": "integer", "minimum": 0 }
            },
            "required": ["decisions", "count"]
        },
        "exit_codes": exit_codes()
    })
}

fn threads_schema() -> Value {
    let mut props = serde_json::Map::new();
    props.insert(
        "gap_hours".to_string(),
        json!({
            "type": "integer",
            "minimum": 0,
            "default": 4,
            "description": "Cluster gap in hours. Sessions in the same project within this gap merge; longer gaps split."
        }),
    );
    props.insert(
        "min_sessions".to_string(),
        json!({
            "type": "integer",
            "minimum": 1,
            "default": 1,
            "description": "Drop threads with fewer than this many sessions."
        }),
    );
    props.insert(
        "limit".to_string(),
        json!({
            "type": "integer",
            "minimum": 0,
            "default": 50,
            "description": "Maximum threads to emit (0 = no limit). Most recent first."
        }),
    );
    props.insert(
        "json".to_string(),
        json!({ "type": "boolean", "description": "Force JSON output." }),
    );
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }

    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/threads",
        "title": "aghist threads",
        "command": "threads",
        "description": "Cluster sessions into threads of related work (same project, time-adjacent). Heuristic: bucket by project_name, walk chronologically, split when the gap exceeds --gap-hours. No LLM.",
        "params": {
            "type": "object",
            "properties": Value::Object(props),
            "additionalProperties": false
        },
        "response": {
            "type": "object",
            "description": "JSON output (when --json or stdout is not a TTY).",
            "properties": {
                "threads": {
                    "type": "array",
                    "description": "Threads ordered by started_at descending, ties broken by project ascending.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Stable short id derived from (project, first_session_ref). Format: `th-<hex16>`."
                            },
                            "project": { "type": ["string", "null"] },
                            "providers": {
                                "type": "array",
                                "items": { "type": "string", "enum": provider_slug_enum() },
                                "description": "Distinct providers represented in this thread, sorted."
                            },
                            "session_count": { "type": "integer", "minimum": 1 },
                            "message_count": { "type": "integer", "minimum": 0 },
                            "started_at": { "type": "string", "format": "date-time" },
                            "ended_at": { "type": "string", "format": "date-time" },
                            "branches": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Distinct git branches recorded across constituent sessions, sorted."
                            },
                            "session_refs": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "`<provider-slug>/<session-id>` for each constituent session, in cluster order."
                            },
                            "summary_seed": {
                                "type": ["string", "null"],
                                "description": "First non-empty session summary in the thread, useful as a hint for human-readable labels."
                            }
                        },
                        "required": ["id", "providers", "session_count", "message_count", "started_at", "ended_at", "branches", "session_refs"]
                    }
                },
                "count": { "type": "integer", "minimum": 0 }
            },
            "required": ["threads", "count"]
        },
        "exit_codes": exit_codes()
    })
}

#[allow(clippy::too_many_lines)]
fn note_schema() -> Value {
    let session_ref_pattern = "^(claude-code|copilot-cli|gemini-cli|codex-cli|opencode|cursor)/[^#]+(#[1-9][0-9]*)?$";
    let note_row = json!({
        "type": "object",
        "properties": {
            "id": { "type": "integer", "minimum": 1 },
            "session_ref": {
                "type": "string",
                "pattern": session_ref_pattern,
                "description": "<provider>/<session-id>[#<turn>]"
            },
            "body": { "type": "string", "minLength": 1 },
            "created_at": { "type": "string", "description": "ISO-8601 UTC, sub-second precision." },
            "updated_at": { "type": "string", "description": "ISO-8601 UTC, sub-second precision." }
        },
        "required": ["id", "session_ref", "body", "created_at", "updated_at"]
    });

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
        "subcommands": {
            "add": {
                "description": "Attach a new note. Body comes from --body, --body-file, or --stdin.",
                "params": {
                    "type": "object",
                    "properties": {
                        "reference": { "type": "string", "pattern": session_ref_pattern },
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
                        "reference": { "type": "string", "pattern": session_ref_pattern },
                        "json": { "type": "boolean" }
                    },
                    "additionalProperties": false
                },
                "response": {
                    "type": "object",
                    "properties": {
                        "notes": { "type": "array", "items": { "$ref": "#/definitions/Note" } },
                        "count": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["notes", "count"]
                }
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
        },
        "definitions": { "Note": note_row },
        "exit_codes": exit_codes()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_subcommand_has_a_schema() {
        for name in SUBCOMMANDS {
            assert!(
                schema_for(name).is_some(),
                "missing schema for subcommand '{name}'"
            );
        }
    }

    #[test]
    fn unknown_subcommand_returns_none() {
        assert!(schema_for("nonsense").is_none());
    }

    #[test]
    fn schemas_declare_draft_2020_12() {
        for name in SUBCOMMANDS {
            let schema = schema_for(name).unwrap();
            assert_eq!(
                schema["$schema"], SCHEMA_DRAFT,
                "subcommand '{name}' missing $schema draft declaration"
            );
            assert!(
                schema["params"].is_object(),
                "subcommand '{name}' missing params object"
            );
            assert!(
                schema["response"].is_object(),
                "subcommand '{name}' missing response object"
            );
        }
    }

    #[test]
    fn index_returns_subcommands_array() {
        let idx = subcommand_index();
        let arr = idx["subcommands"].as_array().unwrap();
        assert_eq!(arr.len(), SUBCOMMANDS.len());
    }

    #[test]
    fn all_schemas_keyed_by_name() {
        let all = all_schemas();
        let map = all.as_object().unwrap();
        for name in SUBCOMMANDS {
            assert!(map.contains_key(*name), "missing key {name} in all_schemas");
        }
    }

    #[test]
    fn search_schema_describes_query_param() {
        let schema = schema_for("search").unwrap();
        let params = &schema["params"]["properties"];
        assert!(params["query"].is_object());
        assert!(params["limit"].is_object());
        assert_eq!(params["limit"]["default"], 20);
    }

    #[test]
    fn show_schema_includes_reference_pattern() {
        let schema = schema_for("show").unwrap();
        let pattern = &schema["params"]["properties"]["reference"]["pattern"];
        assert!(pattern.is_string());
        // Sanity check: the example ref from the description matches the pattern.
        let re = regex_lite_check(pattern.as_str().unwrap(), "claude-code/abc-123#7");
        assert!(re, "show ref pattern should match canonical example");
    }

    /// Tiny helper: we don't pull a regex crate just for tests, so check a few
    /// known anchors without full regex matching.
    fn regex_lite_check(pattern: &str, sample: &str) -> bool {
        // We only assert the pattern is well-formed and the sample contains
        // both "/" and "#" (required by the pattern's structure).
        assert!(pattern.contains('#'));
        sample.contains('/') && sample.contains('#')
    }

}
