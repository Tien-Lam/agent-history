use serde_json::{json, Value};

use super::super::common::{
    exit_codes, filter_params_fragment, provider_slug_enum, source_qualified_session_ref_pattern,
    SCHEMA_DRAFT,
};

pub(in crate::schema) fn todos_schema() -> Value {
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
    props.insert(
        "llm".to_string(),
        json!({
            "type": "boolean",
            "default": false,
            "description": "Route heuristic candidates through an LLM for structured extraction (description/target_session/status_inferred). Requires ANTHROPIC_API_KEY (or AGHIST_LLM_API_KEY)."
        }),
    );
    props.insert(
        "llm_model".to_string(),
        json!({
            "type": "string",
            "description": "Override the LLM model id (default: claude-haiku-4-5-20251001 or AGHIST_LLM_MODEL). Only meaningful with --llm."
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
        "description": "Heuristic scan for unresolved TODOs / follow-ups / open beads-style refs across indexed sessions. With --llm: route candidates through a Claude Messages API call for structured records {description, target_session, status_inferred, ref}. Configured via env (ANTHROPIC_API_KEY, AGHIST_LLM_ENDPOINT, AGHIST_LLM_MODEL).",
        "params": {
            "type": "object",
            "properties": Value::Object(props),
            "additionalProperties": false
        },
        "response": {
            "oneOf": [todos_response_heuristic(), todos_response_llm()]
        },
        "exit_codes": exit_codes()
    })
}

fn todos_response_heuristic() -> Value {
    json!({
        "title": "heuristic mode (default)",
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
                                "pattern": source_qualified_session_ref_pattern(),
                                "description": "Citation ref `<provider>/<session-id>#<turn>` for local sessions, or `<source>:<provider>/<session-id>#<turn>` for remote source sessions. Pass to `aghist show` to inspect."
                            },
                            "source": { "type": "string", "description": "`local` for this host, or a registered remote source name." },
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
                        "required": ["ref", "source", "provider", "session_id", "turn", "kind", "snippet", "role", "timestamp"]
                    }
                },
                "count": { "type": "integer", "minimum": 0 }
            },
            "required": ["todos", "count"]
    })
}

fn todos_response_llm() -> Value {
    json!({
        "title": "llm mode (--llm)",
        "type": "object",
        "description": "Structured records returned when --llm is set. TODOs ordered by started_at descending.",
        "properties": {
            "todos": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "ref": {
                            "type": "string",
                            "pattern": source_qualified_session_ref_pattern(),
                            "description": "Citation ref `<provider>/<session-id>#<turn>` for local sessions, or `<source>:<provider>/<session-id>#<turn>` for remote source sessions."
                        },
                        "source": { "type": "string", "description": "`local` for this host, or a registered remote source name." },
                        "provider": { "type": "string", "enum": provider_slug_enum() },
                        "session_id": { "type": "string" },
                        "turn": { "type": "integer", "minimum": 1 },
                        "description": { "type": "string" },
                        "target_session": {
                            "type": ["string", "null"],
                            "pattern": source_qualified_session_ref_pattern(),
                            "description": "Session ref targeted by the TODO when the model can infer one."
                        },
                        "status_inferred": { "type": "string", "enum": ["open", "done", "unclear"] },
                        "source_snippet": { "type": ["string", "null"] },
                        "source_kind": { "type": ["string", "null"] },
                        "project": { "type": ["string", "null"] },
                        "started_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["ref", "source", "provider", "session_id", "turn", "description", "status_inferred", "started_at"]
                }
            },
            "count": { "type": "integer", "minimum": 0 },
            "mode": { "type": "string", "const": "llm" }
        },
        "required": ["todos", "count", "mode"]
    })
}
