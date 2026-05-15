use serde_json::{json, Value};

use super::common::{exit_codes, filter_params_fragment, provider_slug_enum, SCHEMA_DRAFT};

pub(super) fn todos_schema() -> Value {
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

pub(super) fn decisions_schema() -> Value {
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
    props.insert(
        "llm".to_string(),
        json!({
            "type": "boolean",
            "default": false,
            "description": "Route heuristic candidates through an LLM for structured extraction (summary/rationale/alternatives). Requires ANTHROPIC_API_KEY (or AGHIST_LLM_API_KEY)."
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
        "$id": "aghist:schema/decisions",
        "title": "aghist decisions",
        "command": "decisions",
        "description": "Extract candidate architectural decisions from sessions. Default: heuristic regex/marker scoring. With --llm: route candidates through a Claude Messages API call for structured records {summary, rationale, alternatives, ref}. Configured via env (ANTHROPIC_API_KEY, AGHIST_LLM_ENDPOINT, AGHIST_LLM_MODEL).",
        "params": {
            "type": "object",
            "properties": Value::Object(props),
            "additionalProperties": false
        },
        "response": {
            "oneOf": [decisions_response_heuristic(), decisions_response_llm()]
        },
        "exit_codes": exit_codes()
    })
}

fn decisions_response_heuristic() -> Value {
    json!({
        "title": "heuristic mode (default)",
        "type": "object",
        "description": "Default heuristic output. JSON when --json or stdout is not a TTY.",
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
    })
}

fn decisions_response_llm() -> Value {
    json!({
        "title": "llm mode (--llm)",
        "type": "object",
        "description": "Structured records returned when --llm is set. Decisions ordered by started_at descending.",
        "properties": {
            "decisions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "ref": { "type": "string", "pattern": "^[a-z0-9-]+/.+#[1-9][0-9]*$" },
                        "provider": { "type": "string", "enum": provider_slug_enum() },
                        "session_id": { "type": "string" },
                        "turn": { "type": "integer", "minimum": 1 },
                        "summary": { "type": "string", "description": "One imperative sentence stating what was decided." },
                        "rationale": { "type": "string", "description": "Reasoning, if stated. Empty string when no rationale was given." },
                        "alternatives": { "type": "array", "items": { "type": "string" }, "description": "Explicitly rejected options." },
                        "source_snippet": { "type": ["string", "null"], "description": "The heuristic candidate sentence that anchored this decision." },
                        "project": { "type": ["string", "null"] },
                        "started_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["ref", "provider", "session_id", "turn", "summary", "rationale", "alternatives", "started_at"]
                }
            },
            "count": { "type": "integer", "minimum": 0 },
            "mode": { "type": "string", "const": "llm" }
        },
        "required": ["decisions", "count", "mode"]
    })
}

pub(super) fn threads_schema() -> Value {
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
