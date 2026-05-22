use serde_json::{json, Value};

use super::common::{
    array_schema, closed_object_schema, object_schema, provider_slug_enum,
    provider_slug_enum_nullable, schema_props, source_qualified_citation_ref_pattern,
    source_qualified_session_ref_pattern,
};

pub(crate) fn cursor_meta_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "next_cursor",
                json!({
                    "type": ["string", "null"],
                    "description": "Opaque pagination cursor; pass back with --cursor."
                }),
            ),
            ("total", json!({ "type": "integer", "minimum": 0 })),
        ]),
        &["next_cursor", "total"],
    )
}

pub(crate) fn session_row_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("id", json!({ "type": "string" })),
            (
                "source",
                json!({ "type": "string", "description": "`local` for this host, or a registered remote source name." }),
            ),
            (
                "provider",
                json!({ "type": "string", "enum": provider_slug_enum() }),
            ),
            ("project", json!({ "type": ["string", "null"] })),
            ("branch", json!({ "type": ["string", "null"] })),
            ("summary", json!({ "type": ["string", "null"] })),
            (
                "started_at",
                json!({ "type": "string", "format": "date-time" }),
            ),
            ("message_count", json!({ "type": "integer", "minimum": 0 })),
        ]),
        &["id", "source", "provider", "started_at", "message_count"],
    )
}

pub(crate) fn mcp_session_row_schema() -> Value {
    let mut schema = session_row_schema();
    if let Some(properties) = schema
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
    {
        properties.insert(
            "uri".to_string(),
            json!({ "type": "string", "description": "MCP resource URI for this session." }),
        );
        properties.insert("model".to_string(), json!({ "type": ["string", "null"] }));
        properties.insert(
            "ended_at".to_string(),
            json!({ "type": ["string", "null"], "format": "date-time" }),
        );
    }
    schema
}

pub(crate) fn list_response_schema() -> Value {
    let mut schema = closed_object_schema(
        schema_props([
            ("sessions", array_schema(session_row_schema())),
            ("meta", cursor_meta_schema()),
        ]),
        &["sessions", "meta"],
    );
    schema["description"] = json!("JSON output (when --json or stdout is not a TTY).");
    schema
}

pub(crate) fn source_error_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("source", json!({ "type": "string" })),
            ("error", json!({ "type": "string" })),
        ]),
        &["source", "error"],
    )
}

pub(crate) fn mcp_list_response_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("total", json!({ "type": "integer", "minimum": 0 })),
            ("sessions", array_schema(mcp_session_row_schema())),
            ("source_errors", array_schema(source_error_schema())),
        ]),
        &["total", "sessions", "source_errors"],
    )
}

pub(crate) fn search_meta_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "next_cursor",
                json!({
                    "type": ["string", "null"],
                    "description": "Opaque pagination cursor; pass back with --cursor."
                }),
            ),
            ("total", json!({ "type": "integer", "minimum": 0 })),
            (
                "engine",
                json!({
                    "type": "string",
                    "enum": ["lexical", "hybrid"],
                    "description": "Search engine that produced the results."
                }),
            ),
        ]),
        &["next_cursor", "total", "engine"],
    )
}

pub(crate) fn message_row_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "ref",
                json!({
                    "type": ["string", "null"],
                    "pattern": source_qualified_citation_ref_pattern()
                }),
            ),
            ("uri", json!({ "type": "string" })),
            ("source", json!({ "type": "string" })),
            ("turn", json!({ "type": "integer", "minimum": 1 })),
            ("id", json!({ "type": "string" })),
            (
                "role",
                json!({ "type": "string", "enum": ["user", "assistant", "system", "tool"] }),
            ),
            (
                "timestamp",
                json!({ "type": "string", "format": "date-time" }),
            ),
            ("model", json!({ "type": ["string", "null"] })),
            ("content", array_schema(message_content_block_schema())),
            ("is_target", json!({ "type": "boolean" })),
        ]),
        &[
            "ref",
            "uri",
            "source",
            "turn",
            "id",
            "role",
            "timestamp",
            "model",
            "content",
        ],
    )
}

fn message_content_block_schema() -> Value {
    closed_object_schema(
        schema_props([("type", json!({ "type": "string" })), ("data", json!({}))]),
        &["type"],
    )
}

pub(crate) fn mcp_get_session_response_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("session", mcp_session_row_schema()),
            ("turns", array_schema(message_row_schema())),
        ]),
        &["session", "turns"],
    )
}

pub(crate) fn mcp_get_message_response_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "ref",
                json!({
                    "type": "string",
                    "pattern": source_qualified_citation_ref_pattern()
                }),
            ),
            ("session", mcp_session_row_schema()),
            ("target_turn", json!({ "type": "integer", "minimum": 1 })),
            ("turns", array_schema(message_row_schema())),
        ]),
        &["ref", "session", "target_turn", "turns"],
    )
}

pub(crate) fn search_hit_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "kind",
                json!({
                    "type": "string",
                    "enum": ["message", "note"],
                    "description": "Whether the hit points at a session message or a metadata note."
                }),
            ),
            ("session_id", json!({ "type": "string" })),
            ("message_id", json!({ "type": "string" })),
            ("score", json!({ "type": "number" })),
            ("snippet", json!({ "type": "string" })),
            (
                "provider",
                json!({ "type": ["string", "null"], "enum": provider_slug_enum_nullable() }),
            ),
            ("project", json!({ "type": ["string", "null"] })),
            (
                "started_at",
                json!({ "type": ["string", "null"], "format": "date-time" }),
            ),
            (
                "source",
                json!({
                    "type": "string",
                    "description": "`local` for this host, or a registered remote source name."
                }),
            ),
            (
                "note_id",
                json!({
                    "type": "integer",
                    "description": "Present for note hits; absent for message hits."
                }),
            ),
            (
                "ref",
                json!({
                    "type": "string",
                    "pattern": source_qualified_session_ref_pattern(),
                    "description": "Citation ref for message hits, or the note's stored session ref for note hits."
                }),
            ),
            (
                "turn",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "description": "Present when the hit has been resolved to a 1-based turn number."
                }),
            ),
            (
                "explanation",
                json!({
                    "type": "object",
                    "description": "Present only with --debug-search; Tantivy score explanation tree."
                }),
            ),
        ]),
        &[
            "kind",
            "session_id",
            "message_id",
            "score",
            "snippet",
            "provider",
            "project",
            "started_at",
            "source",
        ],
    )
}

pub(crate) fn search_response_schema() -> Value {
    let mut schema = closed_object_schema(
        schema_props([
            (
                "hits",
                json!({
                    "type": "array",
                    "description": "Hits ordered by score descending then started_at descending.",
                    "items": search_hit_schema()
                }),
            ),
            ("meta", search_meta_schema()),
        ]),
        &["hits", "meta"],
    );
    schema["description"] =
        json!("JSON envelope emitted by `aghist search --json`; watch mode emits one hit object per NDJSON line.");
    schema
}

pub(crate) fn mcp_search_response_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("query", json!({ "type": "string" })),
            ("limit", json!({ "type": "integer", "minimum": 1 })),
            ("total", json!({ "type": "integer", "minimum": 0 })),
            ("hits", array_schema(search_hit_schema())),
            ("source_errors", array_schema(source_error_schema())),
        ]),
        &["query", "limit", "total", "hits", "source_errors"],
    )
}

fn index_error_schema() -> Value {
    json!({
        "oneOf": [
            closed_object_schema(
                schema_props([
                    ("provider", json!({ "type": "string", "enum": provider_slug_enum() })),
                    ("error", json!({ "type": "string" })),
                ]),
                &["provider", "error"],
            ),
            closed_object_schema(
                schema_props([
                    ("source", json!({ "type": "string" })),
                    ("error", json!({ "type": "string" })),
                ]),
                &["source", "error"],
            )
        ]
    })
}

fn embeddings_status_schema() -> Value {
    object_schema(
        schema_props([(
            "status",
            json!({ "type": "string", "enum": ["disabled", "awaiting-consent", "enabled"] }),
        )]),
        &["status"],
    )
}

pub(crate) fn indexing_summary_response_schema() -> Value {
    object_schema(
        schema_props([
            (
                "providers",
                json!({ "type": "array", "items": { "type": "string", "enum": provider_slug_enum() } }),
            ),
            ("sessions_total", json!({ "type": "integer", "minimum": 0 })),
            ("added", json!({ "type": "integer", "minimum": 0 })),
            ("updated", json!({ "type": "integer", "minimum": 0 })),
            ("unchanged", json!({ "type": "integer", "minimum": 0 })),
            ("removed", json!({ "type": "integer", "minimum": 0 })),
            (
                "messages_indexed",
                json!({ "type": "integer", "minimum": 0 }),
            ),
            ("force", json!({ "type": "boolean" })),
            ("index_dir", json!({ "type": "string" })),
            ("duration_ms", json!({ "type": "integer", "minimum": 0 })),
            (
                "errors",
                json!({ "type": "array", "items": index_error_schema() }),
            ),
        ]),
        &[
            "providers",
            "sessions_total",
            "added",
            "updated",
            "unchanged",
            "removed",
            "messages_indexed",
            "force",
            "index_dir",
            "duration_ms",
            "errors",
        ],
    )
}

pub(crate) fn index_response_schema() -> Value {
    let mut schema = indexing_summary_response_schema();
    let properties = schema
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .expect("indexing summary schema has properties");
    properties.insert(
        "embeddings".to_string(),
        with_description(
            embeddings_status_schema(),
            "Status of the optional semantic-embedding pass. Shape varies by status.",
        ),
    );
    let required = schema
        .get_mut("required")
        .and_then(Value::as_array_mut)
        .expect("indexing summary schema has required fields");
    required.push(json!("embeddings"));
    schema
}

pub(crate) fn mcp_reindex_response_schema() -> Value {
    indexing_summary_response_schema()
}

fn with_description(mut schema: Value, description: &str) -> Value {
    schema["description"] = json!(description);
    schema
}

pub(crate) fn health_response_schema() -> Value {
    object_schema(
        schema_props([
            ("ok", json!({ "type": "boolean" })),
            ("checks", array_schema(health_check_schema())),
            ("summary", health_summary_schema()),
            (
                "provider_fidelity",
                array_schema(provider_fidelity_item_schema()),
            ),
        ]),
        &["ok", "checks", "summary", "provider_fidelity"],
    )
}

fn health_check_schema() -> Value {
    object_schema(
        schema_props([
            ("name", json!({ "type": "string" })),
            (
                "status",
                json!({ "type": "string", "enum": ["ok", "warn", "fail"] }),
            ),
            ("message", json!({ "type": "string" })),
            ("hint", json!({ "type": ["string", "null"] })),
        ]),
        &["name", "status", "message"],
    )
}

fn health_summary_schema() -> Value {
    object_schema(
        schema_props([
            ("ok_count", json!({ "type": "integer", "minimum": 0 })),
            ("warn_count", json!({ "type": "integer", "minimum": 0 })),
            ("fail_count", json!({ "type": "integer", "minimum": 0 })),
        ]),
        &["ok_count", "warn_count", "fail_count"],
    )
}

fn provider_parse_stats_schema() -> Value {
    object_schema(
        schema_props([
            ("records_seen", json!({ "type": "integer", "minimum": 0 })),
            ("parse_errors", json!({ "type": "integer", "minimum": 0 })),
            (
                "skipped_records",
                json!({ "type": "integer", "minimum": 0 }),
            ),
            ("empty_content", json!({ "type": "integer", "minimum": 0 })),
        ]),
        &[
            "records_seen",
            "parse_errors",
            "skipped_records",
            "empty_content",
        ],
    )
}

fn provider_block_counts_schema() -> Value {
    object_schema(
        schema_props([
            ("text", json!({ "type": "integer", "minimum": 0 })),
            ("code_block", json!({ "type": "integer", "minimum": 0 })),
            ("tool_use", json!({ "type": "integer", "minimum": 0 })),
            ("tool_result", json!({ "type": "integer", "minimum": 0 })),
            ("thinking", json!({ "type": "integer", "minimum": 0 })),
            ("error", json!({ "type": "integer", "minimum": 0 })),
            ("total", json!({ "type": "integer", "minimum": 0 })),
        ]),
        &[
            "text",
            "code_block",
            "tool_use",
            "tool_result",
            "thinking",
            "error",
            "total",
        ],
    )
}

fn provider_tool_call_fidelity_schema() -> Value {
    object_schema(
        schema_props([
            ("tool_calls", json!({ "type": "integer", "minimum": 0 })),
            ("tool_results", json!({ "type": "integer", "minimum": 0 })),
            ("paired", json!({ "type": "integer", "minimum": 0 })),
            ("unpaired_calls", json!({ "type": "integer", "minimum": 0 })),
            ("orphan_results", json!({ "type": "integer", "minimum": 0 })),
            ("empty_names", json!({ "type": "integer", "minimum": 0 })),
            ("empty_call_ids", json!({ "type": "integer", "minimum": 0 })),
            (
                "empty_result_ids",
                json!({ "type": "integer", "minimum": 0 }),
            ),
            (
                "invalid_json_args",
                json!({ "type": "integer", "minimum": 0 }),
            ),
            (
                "success_results",
                json!({ "type": "integer", "minimum": 0 }),
            ),
            (
                "failure_results",
                json!({ "type": "integer", "minimum": 0 }),
            ),
        ]),
        &[
            "tool_calls",
            "tool_results",
            "paired",
            "unpaired_calls",
            "orphan_results",
            "empty_names",
            "empty_call_ids",
            "empty_result_ids",
            "invalid_json_args",
            "success_results",
            "failure_results",
        ],
    )
}

fn provider_fidelity_item_schema() -> Value {
    object_schema(
        schema_props([
            ("label", json!({ "type": "string" })),
            ("provider", json!({ "type": "string" })),
            ("session_count", json!({ "type": "integer", "minimum": 0 })),
            ("message_count", json!({ "type": "integer", "minimum": 0 })),
            ("parse", provider_parse_stats_schema()),
            ("blocks", provider_block_counts_schema()),
            ("tool_call_fidelity", provider_tool_call_fidelity_schema()),
        ]),
        &[
            "label",
            "provider",
            "session_count",
            "message_count",
            "parse",
            "blocks",
            "tool_call_fidelity",
        ],
    )
}
