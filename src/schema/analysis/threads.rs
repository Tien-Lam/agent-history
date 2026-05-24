use serde_json::{json, Value};

use crate::schema_fragments::{
    ANALYSIS_LIMIT_MAX, ANALYSIS_THREADS_LIMIT_DEFAULT, ANALYSIS_THREADS_LLM_MAX_SESSIONS_DEFAULT,
    ANALYSIS_THREADS_LLM_MAX_SESSIONS_MAX,
};

use super::super::common::{
    closed_object_schema, exit_codes, provider_slug_enum, schema_props_with_filters,
    source_qualified_session_only_ref_pattern, SCHEMA_DRAFT,
};

pub(in crate::schema) fn threads_schema() -> Value {
    let params = closed_object_schema(
        schema_props_with_filters([
            (
                "gap_hours",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "default": 4,
                    "description": "Cluster gap in hours. Sessions in the same project within this gap merge; longer gaps split."
                }),
            ),
            (
                "min_sessions",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "default": 1,
                    "description": "Drop threads with fewer than this many sessions."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": ANALYSIS_LIMIT_MAX,
                    "default": ANALYSIS_THREADS_LIMIT_DEFAULT,
                    "description": "Maximum threads to emit. Most recent first."
                }),
            ),
            (
                "json",
                json!({ "type": "boolean", "description": "Force JSON output." }),
            ),
            (
                "llm",
                json!({
                    "type": "boolean",
                    "default": false,
                    "description": "Route session digests through an LLM for semantic topic clustering across project boundaries. Requires ANTHROPIC_API_KEY (or AGHIST_LLM_API_KEY)."
                }),
            ),
            (
                "llm_model",
                json!({
                    "type": "string",
                    "description": "Override the LLM model id (default: claude-haiku-4-5-20251001 or AGHIST_LLM_MODEL). Only meaningful with --llm."
                }),
            ),
            (
                "llm_max_sessions",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": ANALYSIS_THREADS_LLM_MAX_SESSIONS_MAX,
                    "default": ANALYSIS_THREADS_LLM_MAX_SESSIONS_DEFAULT,
                    "description": "Cap on session digests sent to the LLM. Only meaningful with --llm."
                }),
            ),
        ]),
        &[],
    );

    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/threads",
        "title": "aghist threads",
        "command": "threads",
        "description": "Cluster sessions into threads of related work. Default heuristic: bucket by project_name, walk chronologically, split when the gap exceeds --gap-hours. With --llm: route session digests through a Claude Messages API call for semantic topic clustering across projects.",
        "params": params,
        "response": {
            "oneOf": [threads_response_heuristic(), threads_response_llm()]
        },
        "exit_codes": exit_codes()
    })
}

fn threads_response_heuristic() -> Value {
    json!({
        "title": "heuristic mode (default)",
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
                                "items": { "type": "string", "pattern": source_qualified_session_only_ref_pattern() },
                                "description": "`<provider-slug>/<session-id>` for local sessions, or `<source>:<provider-slug>/<session-id>` for remote source sessions, in cluster order."
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
    })
}

fn threads_response_llm() -> Value {
    json!({
        "title": "llm mode (--llm)",
        "type": "object",
        "description": "Structured topic clusters returned when --llm is set.",
        "properties": {
            "threads": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Stable short id derived from (topic_summary, first_member_ref). Format: `th-<hex16>`."
                        },
                        "topic_summary": { "type": "string" },
                        "member_refs": {
                            "type": "array",
                            "items": { "type": "string", "pattern": source_qualified_session_only_ref_pattern() },
                            "description": "`<provider-slug>/<session-id>` for local sessions, or `<source>:<provider-slug>/<session-id>` for remote source sessions."
                        },
                        "time_span": {
                            "type": "object",
                            "properties": {
                                "start": { "type": "string", "format": "date-time" },
                                "end": { "type": "string", "format": "date-time" }
                            },
                            "required": ["start", "end"]
                        },
                        "providers": {
                            "type": "array",
                            "items": { "type": "string", "enum": provider_slug_enum() }
                        },
                        "projects": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "branches": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "message_count": { "type": "integer", "minimum": 0 }
                    },
                    "required": ["id", "topic_summary", "member_refs", "time_span", "providers", "projects", "branches", "message_count"]
                }
            },
            "count": { "type": "integer", "minimum": 0 },
            "mode": { "type": "string", "const": "llm" }
        },
        "required": ["threads", "count", "mode"]
    })
}
