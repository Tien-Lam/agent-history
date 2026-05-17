use serde_json::{json, Value};

use crate::model::Provider;

pub const LIST_LIMIT_DEFAULT: usize = 20;
pub const SEARCH_LIMIT_DEFAULT: usize = 20;
pub const SEARCH_WATCH_INTERVAL_MS_DEFAULT: u64 = 2_000;
pub const SEARCH_WATCH_ITERATIONS_DEFAULT: u32 = 0;
pub const SEARCH_HYBRID_WEIGHT_DEFAULT: f32 = 0.0;
pub const SHOW_INCLUDE_CONTEXT_DEFAULT: u32 = 0;

pub const MCP_SEARCH_LIMIT_MAX: usize = 200;
pub const MCP_LIST_LIMIT_DEFAULT: usize = 50;
pub const MCP_LIST_LIMIT_MAX: usize = 1_000;
pub const MCP_INCLUDE_CONTEXT_DEFAULT: usize = 0;
pub const MCP_INCLUDE_CONTEXT_MAX: usize = 100;

fn provider_slug_pattern() -> String {
    Provider::all()
        .iter()
        .map(|provider| provider.slug())
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn source_qualified_session_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+(#[1-9][0-9]*)?$",
        provider_slug_pattern()
    )
}

pub(crate) fn source_qualified_session_only_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+$",
        provider_slug_pattern()
    )
}

pub(crate) fn source_qualified_citation_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+#[1-9][0-9]*$",
        provider_slug_pattern()
    )
}

pub(crate) fn provider_slug_enum() -> Value {
    json!(Provider::all().iter().map(|p| p.slug()).collect::<Vec<_>>())
}

pub(crate) fn provider_slug_enum_nullable() -> Value {
    let mut slugs: Vec<Value> = Provider::all().iter().map(|p| json!(p.slug())).collect();
    slugs.push(Value::Null);
    Value::Array(slugs)
}

pub(crate) fn cursor_meta_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "next_cursor": {
                "type": ["string", "null"],
                "description": "Opaque pagination cursor; pass back with --cursor."
            },
            "total": { "type": "integer", "minimum": 0 }
        },
        "required": ["next_cursor", "total"],
        "additionalProperties": false
    })
}

pub(crate) fn session_row_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "source": { "type": "string", "description": "`local` for this host, or a registered remote source name." },
            "provider": { "type": "string", "enum": provider_slug_enum() },
            "project": { "type": ["string", "null"] },
            "branch": { "type": ["string", "null"] },
            "summary": { "type": ["string", "null"] },
            "started_at": { "type": "string", "format": "date-time" },
            "message_count": { "type": "integer", "minimum": 0 }
        },
        "required": ["id", "source", "provider", "started_at", "message_count"],
        "additionalProperties": false
    })
}

pub(crate) fn list_response_schema() -> Value {
    json!({
        "type": "object",
        "description": "JSON output (when --json or stdout is not a TTY).",
        "properties": {
            "sessions": { "type": "array", "items": session_row_schema() },
            "meta": cursor_meta_schema()
        },
        "required": ["sessions", "meta"],
        "additionalProperties": false
    })
}

pub(crate) fn search_meta_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "next_cursor": {
                "type": ["string", "null"],
                "description": "Opaque pagination cursor; pass back with --cursor."
            },
            "total": { "type": "integer", "minimum": 0 },
            "engine": {
                "type": "string",
                "enum": ["lexical", "hybrid"],
                "description": "Search engine that produced the results."
            }
        },
        "required": ["next_cursor", "total", "engine"],
        "additionalProperties": false
    })
}

pub(crate) fn search_hit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["message", "note"],
                "description": "Whether the hit points at a session message or a metadata note."
            },
            "session_id": { "type": "string" },
            "message_id": { "type": "string" },
            "score": { "type": "number" },
            "snippet": { "type": "string" },
            "provider": { "type": ["string", "null"], "enum": provider_slug_enum_nullable() },
            "project": { "type": ["string", "null"] },
            "started_at": { "type": ["string", "null"], "format": "date-time" },
            "source": {
                "type": "string",
                "description": "`local` for this host, or a registered remote source name."
            },
            "note_id": {
                "type": "integer",
                "description": "Present for note hits; absent for message hits."
            },
            "ref": {
                "type": "string",
                "pattern": source_qualified_session_ref_pattern(),
                "description": "Citation ref for message hits, or the note's stored session ref for note hits."
            },
            "turn": {
                "type": "integer",
                "minimum": 1,
                "description": "Present when the hit has been resolved to a 1-based turn number."
            },
            "explanation": {
                "type": "object",
                "description": "Present only with --debug-search; Tantivy score explanation tree."
            }
        },
        "required": [
            "kind",
            "session_id",
            "message_id",
            "score",
            "snippet",
            "provider",
            "project",
            "started_at",
            "source"
        ],
        "additionalProperties": false
    })
}

pub(crate) fn search_response_schema() -> Value {
    json!({
        "type": "object",
        "description": "JSON envelope emitted by `aghist search --json`; watch mode emits one hit object per NDJSON line.",
        "properties": {
            "hits": {
                "type": "array",
                "description": "Hits ordered by score descending then started_at descending.",
                "items": search_hit_schema()
            },
            "meta": search_meta_schema()
        },
        "required": ["hits", "meta"],
        "additionalProperties": false
    })
}
