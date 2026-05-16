use serde_json::{json, Value};

use super::super::common::{
    exit_codes, filter_params_fragment, source_qualified_session_ref_pattern, SCHEMA_DRAFT,
};

pub(in crate::schema) fn track_schema() -> Value {
    let mut props = serde_json::Map::new();
    props.insert(
        "topic".to_string(),
        json!({
            "type": "string",
            "minLength": 1,
            "description": "Free-text topic to track across sessions."
        }),
    );
    props.insert(
        "limit".to_string(),
        json!({
            "type": "integer",
            "minimum": 0,
            "default": 50,
            "description": "Maximum matching sessions to send to the LLM after chronological sorting (0 = no cap)."
        }),
    );
    props.insert(
        "json".to_string(),
        json!({ "type": "boolean", "description": "Force JSON output." }),
    );
    props.insert(
        "llm_model".to_string(),
        json!({
            "type": "string",
            "description": "Override the LLM model id (default: claude-haiku-4-5-20251001 or AGHIST_LLM_MODEL)."
        }),
    );
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }

    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/track",
        "title": "aghist track",
        "command": "track",
        "description": "Track how a topic evolved across local and remote-source sessions. Finds sessions that mention the topic by keyword, extracts relevant snippets, and asks an LLM for a chronological change timeline. Requires ANTHROPIC_API_KEY (or AGHIST_LLM_API_KEY).",
        "params": {
            "type": "object",
            "properties": Value::Object(props),
            "required": ["topic"],
            "additionalProperties": false
        },
        "response": {
            "type": "object",
            "description": "JSON output (when --json or stdout is not a TTY).",
            "properties": {
                "topic": { "type": "string" },
                "sessions_scanned": { "type": "integer", "minimum": 0 },
                "timeline": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "session_ref": {
                                "type": "string",
                                "pattern": source_qualified_session_ref_pattern(),
                                "description": "`<provider-slug>/<session-id>` for local sessions, or `<source>:<provider-slug>/<session-id>` for remote source sessions."
                            },
                            "date": {
                                "type": "string",
                                "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$"
                            },
                            "event": { "type": "string" },
                            "direction": {
                                "type": "string",
                                "enum": ["introduced", "revised", "confirmed", "dropped"]
                            }
                        },
                        "required": ["session_ref", "date", "event", "direction"]
                    }
                }
            },
            "required": ["topic", "sessions_scanned", "timeline"]
        },
        "exit_codes": exit_codes()
    })
}
