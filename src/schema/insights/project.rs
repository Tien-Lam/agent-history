use serde_json::{json, Value};

use super::super::common::{exit_codes, filter_params_fragment, SCHEMA_DRAFT};
use super::{
    decisions_array_schema, limits_schema, time_of_day_schema, todos_array_schema,
    token_usage_summary_schema, top_files_array_schema,
};

fn project_meta_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "limits": limits_schema(&["decisions", "todos", "threads", "files"]),
            "decisions_total": { "type": "integer", "minimum": 0 },
            "todos_total": { "type": "integer", "minimum": 0 },
            "threads_total": { "type": "integer", "minimum": 0 },
            "files_total": { "type": "integer", "minimum": 0 },
            "thread_gap_hours": { "type": "integer", "minimum": 0 },
            "decisions_threshold": { "type": "number" }
        },
        "required": ["limits", "decisions_total", "todos_total", "threads_total", "files_total", "thread_gap_hours", "decisions_threshold"]
    })
}

fn project_response_schema() -> Value {
    json!({
        "type": "object",
        "description": "JSON output (when --json or stdout is not a TTY).",
        "properties": {
            "query": {
                "type": "string",
                "description": "The literal `<name>` query, after trimming."
            },
            "matched_projects": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Distinct project names whose sessions matched the query."
            },
            "session_count": { "type": "integer", "minimum": 0 },
            "message_count": { "type": "integer", "minimum": 0 },
            "started_at": { "type": ["string", "null"], "format": "date-time" },
            "ended_at": { "type": ["string", "null"], "format": "date-time" },
            "token_usage": token_usage_summary_schema("project"),
            "decisions": decisions_array_schema(),
            "todos": todos_array_schema(),
            "threads": {
                "type": "array",
                "description": "Cross-session work threads (`aghist threads` output, scoped to this project)."
            },
            "top_files": top_files_array_schema(),
            "time_of_day": time_of_day_schema(),
            "meta": project_meta_schema()
        },
        "required": ["query", "matched_projects", "session_count", "message_count", "token_usage", "decisions", "todos", "threads", "top_files", "time_of_day", "meta"]
    })
}

pub(in crate::schema) fn project_schema() -> Value {
    let mut props = serde_json::Map::new();
    props.insert(
        "name".to_string(),
        json!({
            "type": "string",
            "minLength": 1,
            "description": "Project name. Matched as a case-insensitive substring against each session's project_name."
        }),
    );
    props.insert(
        "decisions".to_string(),
        json!({
            "type": "integer",
            "minimum": 0,
            "default": 5,
            "description": "Cap the decisions section (0 = no cap). Raw count remains in meta.decisions_total."
        }),
    );
    props.insert(
        "todos".to_string(),
        json!({
            "type": "integer",
            "minimum": 0,
            "default": 10,
            "description": "Cap the todos section (0 = no cap). Raw count remains in meta.todos_total."
        }),
    );
    props.insert(
        "threads".to_string(),
        json!({
            "type": "integer",
            "minimum": 0,
            "default": 5,
            "description": "Cap the threads section (0 = no cap). Raw count remains in meta.threads_total."
        }),
    );
    props.insert(
        "files".to_string(),
        json!({
            "type": "integer",
            "minimum": 0,
            "default": 10,
            "description": "Cap the top-files section (0 = no cap). Raw count remains in meta.files_total."
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
        "$id": "aghist:schema/project",
        "title": "aghist project",
        "command": "project",
        "description": "Per-project productivity dashboard. Aggregates one project's sessions into session/message counts, token usage (with cost when known), heuristic decisions/TODOs, threads, top files touched, and a 24-bucket UTC time-of-day histogram. Heuristics reuse `aghist decisions/todos/threads/usage` — no LLM. Empty result exits 3.",
        "params": {
            "type": "object",
            "properties": Value::Object(props),
            "required": ["name"],
            "additionalProperties": false
        },
        "response": project_response_schema(),
        "exit_codes": exit_codes()
    })
}
