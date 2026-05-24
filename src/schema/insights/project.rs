use serde_json::{json, Value};

use crate::schema_fragments::REPORT_SECTION_LIMIT_MAX;

use super::super::common::{
    closed_object_schema, exit_codes, schema_props_with_filters, SCHEMA_DRAFT,
};
use super::{
    decisions_array_schema, limits_schema, threads_array_schema, time_of_day_schema,
    todos_array_schema, token_usage_summary_schema, top_files_array_schema,
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
            "threads": threads_array_schema("project"),
            "top_files": top_files_array_schema(),
            "time_of_day": time_of_day_schema(),
            "meta": project_meta_schema()
        },
        "required": ["query", "matched_projects", "session_count", "message_count", "token_usage", "decisions", "todos", "threads", "top_files", "time_of_day", "meta"]
    })
}

pub(in crate::schema) fn project_schema() -> Value {
    let params = closed_object_schema(
        schema_props_with_filters([
            (
                "name",
                json!({
                    "type": "string",
                    "minLength": 1,
                    "description": "Project name. Matched as a case-insensitive substring against each session's project_name."
                }),
            ),
            (
                "decisions",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": REPORT_SECTION_LIMIT_MAX,
                    "default": 5,
                    "description": "Cap the decisions section. Raw count remains in meta.decisions_total."
                }),
            ),
            (
                "todos",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": REPORT_SECTION_LIMIT_MAX,
                    "default": 10,
                    "description": "Cap the todos section. Raw count remains in meta.todos_total."
                }),
            ),
            (
                "threads",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": REPORT_SECTION_LIMIT_MAX,
                    "default": 5,
                    "description": "Cap the threads section. Raw count remains in meta.threads_total."
                }),
            ),
            (
                "files",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": REPORT_SECTION_LIMIT_MAX,
                    "default": 10,
                    "description": "Cap the top-files section. Raw count remains in meta.files_total."
                }),
            ),
            (
                "json",
                json!({
                    "type": "boolean",
                    "description": "Force JSON output (default: JSON on pipe, table on TTY)."
                }),
            ),
        ]),
        &["name"],
    );

    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/project",
        "title": "aghist project",
        "command": "project",
        "description": "Per-project productivity dashboard. Aggregates one project's sessions into session/message counts, token usage (with cost when known), heuristic decisions/TODOs, threads, top files touched, and a 24-bucket UTC time-of-day histogram. Heuristics reuse `aghist decisions/todos/threads/usage` — no LLM. Empty result exits 3.",
        "params": params,
        "response": project_response_schema(),
        "exit_codes": exit_codes()
    })
}
