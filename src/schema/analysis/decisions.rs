use serde_json::{json, Value};

use super::super::common::{
    exit_codes, filter_params_fragment, provider_slug_enum, SCHEMA_DRAFT,
    SOURCE_QUALIFIED_SESSION_REF_PATTERN,
};

pub(in crate::schema) fn decisions_schema() -> Value {
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
                            "pattern": SOURCE_QUALIFIED_SESSION_REF_PATTERN,
                            "description": "Citation ref `<provider>/<session-id>#<turn>` for local sessions, or `<source>:<provider>/<session-id>#<turn>` for remote source sessions."
                        },
                        "source": { "type": "string", "description": "`local` for this host, or a registered remote source name." },
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
                    "required": ["ref", "source", "provider", "session_id", "turn", "role", "score", "markers", "snippet", "timestamp", "started_at"]
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
