use serde_json::{json, Value};

use super::super::common::{
    closed_object_schema, exit_codes, object_schema, schema_props, schema_props_with_filters,
    source_qualified_session_only_ref_pattern, SCHEMA_DRAFT,
};

pub(in crate::schema) fn track_schema() -> Value {
    let params = closed_object_schema(
        schema_props_with_filters([
            (
                "topic",
                json!({
                    "type": "string",
                    "minLength": 1,
                    "description": "Free-text topic to track across sessions."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 0,
                    "default": 50,
                    "description": "Maximum matching sessions to send to the LLM after chronological sorting (0 = no cap)."
                }),
            ),
            (
                "json",
                json!({ "type": "boolean", "description": "Force JSON output." }),
            ),
            (
                "llm_model",
                json!({
                    "type": "string",
                    "description": "Override the LLM model id (default: claude-haiku-4-5-20251001 or AGHIST_LLM_MODEL)."
                }),
            ),
        ]),
        &["topic"],
    );

    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/track",
        "title": "aghist track",
        "command": "track",
        "description": "Track how a topic evolved across local and remote-source sessions. Finds sessions that mention the topic by keyword, extracts relevant snippets, and asks an LLM for a chronological change timeline. Requires ANTHROPIC_API_KEY (or AGHIST_LLM_API_KEY).",
        "params": params,
        "response": track_response_schema(),
        "exit_codes": exit_codes()
    })
}

fn track_response_schema() -> Value {
    let mut schema = object_schema(
        schema_props([
            ("topic", json!({ "type": "string" })),
            (
                "sessions_scanned",
                json!({ "type": "integer", "minimum": 0 }),
            ),
            (
                "timeline",
                json!({
                    "type": "array",
                    "items": object_schema(
                        schema_props([
                            (
                                "session_ref",
                                json!({
                                    "type": "string",
                                    "pattern": source_qualified_session_only_ref_pattern(),
                                    "description": "`<provider-slug>/<session-id>` for local sessions, or `<source>:<provider-slug>/<session-id>` for remote source sessions."
                                }),
                            ),
                            (
                                "date",
                                json!({
                                    "type": "string",
                                    "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$"
                                }),
                            ),
                            ("event", json!({ "type": "string" })),
                            (
                                "direction",
                                json!({
                                    "type": "string",
                                    "enum": ["introduced", "revised", "confirmed", "dropped"]
                                }),
                            ),
                        ]),
                        &["session_ref", "date", "event", "direction"],
                    )
                }),
            ),
        ]),
        &["topic", "sessions_scanned", "timeline"],
    );
    schema["description"] = json!("JSON output (when --json or stdout is not a TTY).");
    schema
}
