use serde_json::{json, Value};

use super::super::common::{exit_codes, filter_params_fragment, SCHEMA_DRAFT};
use super::{
    decisions_array_schema, limits_schema, todos_array_schema, token_usage_summary_schema,
};

fn top_projects_array_schema() -> Value {
    json!({
        "type": "array",
        "description": "Most active projects in the window, sorted by message_count desc.",
        "items": {
            "type": "object",
            "properties": {
                "project": { "type": "string" },
                "session_count": { "type": "integer", "minimum": 0 },
                "message_count": { "type": "integer", "minimum": 0 },
                "input_tokens": { "type": "integer", "minimum": 0 },
                "output_tokens": { "type": "integer", "minimum": 0 },
                "total_tokens": { "type": "integer", "minimum": 0 },
                "cost_usd": { "type": ["number", "null"] }
            },
            "required": ["project", "session_count", "message_count", "input_tokens", "output_tokens", "total_tokens", "cost_usd"]
        }
    })
}

fn report_meta_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "limits": limits_schema(&["top_projects", "decisions", "todos", "threads"]),
            "projects_total": { "type": "integer", "minimum": 0 },
            "decisions_total": { "type": "integer", "minimum": 0 },
            "todos_total": { "type": "integer", "minimum": 0 },
            "threads_total": { "type": "integer", "minimum": 0 },
            "thread_gap_hours": { "type": "integer", "minimum": 0 },
            "decisions_threshold": { "type": "number" }
        },
        "required": ["limits", "projects_total", "decisions_total", "todos_total", "threads_total", "thread_gap_hours", "decisions_threshold"]
    })
}

fn report_response_schema() -> Value {
    json!({
        "type": "object",
        "description": "JSON envelope (when `json:true`). Without `json`, the response is a Markdown document.",
        "properties": {
            "window": {
                "type": "object",
                "properties": {
                    "started_at": { "type": "string", "format": "date-time" },
                    "ended_at": { "type": "string", "format": "date-time" },
                    "days": { "type": "integer", "minimum": 1 }
                },
                "required": ["started_at", "ended_at", "days"]
            },
            "session_count": { "type": "integer", "minimum": 0 },
            "message_count": { "type": "integer", "minimum": 0 },
            "project_count": { "type": "integer", "minimum": 0 },
            "token_usage": token_usage_summary_schema("window"),
            "top_projects": top_projects_array_schema(),
            "decisions": decisions_array_schema(),
            "todos": todos_array_schema(),
            "threads": {
                "type": "array",
                "description": "Cross-session work threads in the window (`aghist threads` output, scoped to the window)."
            },
            "meta": report_meta_schema()
        },
        "required": ["window", "session_count", "message_count", "project_count", "token_usage", "top_projects", "decisions", "todos", "threads", "meta"]
    })
}

pub(in crate::schema) fn report_schema() -> Value {
    let mut props = serde_json::Map::new();
    props.insert(
        "days".to_string(),
        json!({
            "type": "integer",
            "minimum": 1,
            "default": 7,
            "description": "Window length in days. Mutually exclusive with `week`/`month`."
        }),
    );
    props.insert(
        "week".to_string(),
        json!({
            "type": "boolean",
            "description": "Shorthand for `days=7`. Mutually exclusive with `days`/`month`."
        }),
    );
    props.insert(
        "month".to_string(),
        json!({
            "type": "boolean",
            "description": "Shorthand for `days=30`. Mutually exclusive with `days`/`week`."
        }),
    );
    props.insert(
        "top_projects".to_string(),
        json!({
            "type": "integer",
            "minimum": 0,
            "default": 3,
            "description": "Cap the top-projects section (0 = no cap). Raw count remains in meta.projects_total."
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
        "json".to_string(),
        json!({
            "type": "boolean",
            "description": "Emit the structured JSON envelope instead of Markdown. Default is Markdown for both TTY and pipe."
        }),
    );
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }

    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/report",
        "title": "aghist report",
        "command": "report",
        "description": "Cross-project weekly summary suitable for journals or reviews. Aggregates a window of activity (default last 7 days) across every provider into top active projects, decision count, open TODOs, and completed work threads. Heuristics reuse `aghist decisions/todos/threads/usage` — no LLM. Default output is Markdown; `json:true` emits the structured envelope. Empty result exits 3.",
        "params": {
            "type": "object",
            "properties": Value::Object(props),
            "required": [],
            "additionalProperties": false
        },
        "response": report_response_schema(),
        "exit_codes": exit_codes()
    })
}
