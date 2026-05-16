use serde_json::{json, Value};

use super::common::{exit_codes, filter_params_fragment, provider_slug_enum, SCHEMA_DRAFT};

mod usage;

pub(super) use usage::usage_schema;

fn token_usage_summary_schema(scope: &str) -> Value {
    json!({
        "type": "object",
        "properties": {
            "input_tokens": { "type": "integer", "minimum": 0 },
            "output_tokens": { "type": "integer", "minimum": 0 },
            "cache_read_tokens": { "type": "integer", "minimum": 0 },
            "cache_write_tokens": { "type": "integer", "minimum": 0 },
            "total_tokens": { "type": "integer", "minimum": 0 },
            "cost_usd": {
                "type": ["number", "null"],
                "description": format!("USD across the {scope}, or null if any session uses an unpriced model.")
            }
        },
        "required": ["input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens", "total_tokens", "cost_usd"]
    })
}

fn decision_candidate_item_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "ref": { "type": "string" },
            "provider": { "type": "string", "enum": provider_slug_enum() },
            "session_id": { "type": "string" },
            "turn": { "type": "integer", "minimum": 1 },
            "score": { "type": "number" },
            "markers": { "type": "array", "items": { "type": "string" } },
            "snippet": { "type": "string" },
            "timestamp": { "type": "string", "format": "date-time" }
        },
        "required": ["ref", "provider", "session_id", "turn", "score", "markers", "snippet", "timestamp"]
    })
}

fn todo_candidate_item_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "ref": { "type": "string" },
            "provider": { "type": "string", "enum": provider_slug_enum() },
            "session_id": { "type": "string" },
            "turn": { "type": "integer", "minimum": 1 },
            "kind": {
                "type": "string",
                "enum": ["todo", "follow_up", "come_back_to", "we_should", "bd_ref"]
            },
            "snippet": { "type": "string" },
            "timestamp": { "type": "string", "format": "date-time" },
            "bd_id": { "type": ["string", "null"] }
        },
        "required": ["ref", "provider", "session_id", "turn", "kind", "snippet", "timestamp"]
    })
}

fn decisions_array_schema() -> Value {
    json!({
        "type": "array",
        "description": "Top-scoring decision candidates, sorted by score desc.",
        "items": decision_candidate_item_schema()
    })
}

fn todos_array_schema() -> Value {
    json!({
        "type": "array",
        "description": "Open TODOs / follow-ups / bd refs, newest first.",
        "items": todo_candidate_item_schema()
    })
}

fn top_files_array_schema() -> Value {
    json!({
        "type": "array",
        "description": "Files most often referenced by tool calls. Counts derive from top-level `file_path`/`path`/`notebook_path`/`filename`/`target_file` keys in tool-call JSON.",
        "items": {
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "count": { "type": "integer", "minimum": 1 }
            },
            "required": ["path", "count"]
        }
    })
}

fn time_of_day_schema() -> Value {
    json!({
        "type": "array",
        "description": "24-element UTC histogram of message counts. Index = hour (0..23).",
        "minItems": 24,
        "maxItems": 24,
        "items": { "type": "integer", "minimum": 0 }
    })
}

fn limits_schema(names: &[&str]) -> Value {
    let mut props = serde_json::Map::new();
    for name in names {
        props.insert(
            (*name).to_string(),
            json!({ "type": "integer", "minimum": 0 }),
        );
    }
    json!({
        "type": "object",
        "properties": Value::Object(props),
        "required": names
    })
}

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

pub(super) fn project_schema() -> Value {
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

pub(super) fn report_schema() -> Value {
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
