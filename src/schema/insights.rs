use serde_json::{json, Value};

use super::common::{provider_slug_enum, SOURCE_QUALIFIED_SESSION_REF_PATTERN};

mod project;
mod report;
mod usage;

pub(super) use project::project_schema;
pub(super) use report::report_schema;
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
            "ref": {
                "type": "string",
                "pattern": SOURCE_QUALIFIED_SESSION_REF_PATTERN,
                "description": "Citation ref `<provider>/<session-id>#<turn>` for local sessions, or `<source>:<provider>/<session-id>#<turn>` for remote source sessions."
            },
            "source": { "type": "string", "description": "`local` for this host, or a registered remote source name." },
            "provider": { "type": "string", "enum": provider_slug_enum() },
            "session_id": { "type": "string" },
            "turn": { "type": "integer", "minimum": 1 },
            "score": { "type": "number" },
            "markers": { "type": "array", "items": { "type": "string" } },
            "snippet": { "type": "string" },
            "timestamp": { "type": "string", "format": "date-time" }
        },
        "required": ["ref", "source", "provider", "session_id", "turn", "score", "markers", "snippet", "timestamp"]
    })
}

fn todo_candidate_item_schema() -> Value {
    json!({
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
            "kind": {
                "type": "string",
                "enum": ["todo", "follow_up", "come_back_to", "we_should", "bd_ref"]
            },
            "snippet": { "type": "string" },
            "timestamp": { "type": "string", "format": "date-time" },
            "bd_id": { "type": ["string", "null"] }
        },
        "required": ["ref", "source", "provider", "session_id", "turn", "kind", "snippet", "timestamp"]
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
