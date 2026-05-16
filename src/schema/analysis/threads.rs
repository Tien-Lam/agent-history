use serde_json::{json, Value};

use super::super::common::{exit_codes, filter_params_fragment, provider_slug_enum, SCHEMA_DRAFT};

pub(in crate::schema) fn threads_schema() -> Value {
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
