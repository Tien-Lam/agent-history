use serde_json::{json, Value};

use super::super::common::{exit_codes, filter_params_fragment, provider_slug_enum, SCHEMA_DRAFT};

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
