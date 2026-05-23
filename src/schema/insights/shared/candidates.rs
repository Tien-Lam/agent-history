use serde_json::{json, Value};

use crate::schema::common::{
    array_schema, object_schema, provider_slug_enum, schema_props,
    source_qualified_citation_ref_pattern,
};

fn decision_candidate_item_schema() -> Value {
    object_schema(
        schema_props([
            (
                "ref",
                json!({
                    "type": "string",
                    "pattern": source_qualified_citation_ref_pattern(),
                    "description": "Citation ref `<provider>/<session-id>#<turn>` for local sessions, or `<source>:<provider>/<session-id>#<turn>` for remote source sessions."
                }),
            ),
            (
                "source",
                json!({ "type": "string", "description": "`local` for this host, or a registered remote source name." }),
            ),
            (
                "provider",
                json!({ "type": "string", "enum": provider_slug_enum() }),
            ),
            ("session_id", json!({ "type": "string" })),
            ("turn", json!({ "type": "integer", "minimum": 1 })),
            ("score", json!({ "type": "number" })),
            ("markers", array_schema(json!({ "type": "string" }))),
            ("snippet", json!({ "type": "string" })),
            (
                "timestamp",
                json!({ "type": "string", "format": "date-time" }),
            ),
        ]),
        &[
            "ref",
            "source",
            "provider",
            "session_id",
            "turn",
            "score",
            "markers",
            "snippet",
            "timestamp",
        ],
    )
}

fn todo_candidate_item_schema() -> Value {
    object_schema(
        schema_props([
            (
                "ref",
                json!({
                    "type": "string",
                    "pattern": source_qualified_citation_ref_pattern(),
                    "description": "Citation ref `<provider>/<session-id>#<turn>` for local sessions, or `<source>:<provider>/<session-id>#<turn>` for remote source sessions."
                }),
            ),
            (
                "source",
                json!({ "type": "string", "description": "`local` for this host, or a registered remote source name." }),
            ),
            (
                "provider",
                json!({ "type": "string", "enum": provider_slug_enum() }),
            ),
            ("session_id", json!({ "type": "string" })),
            ("turn", json!({ "type": "integer", "minimum": 1 })),
            (
                "kind",
                json!({
                    "type": "string",
                    "enum": ["todo", "follow_up", "come_back_to", "we_should", "bd_ref"]
                }),
            ),
            ("snippet", json!({ "type": "string" })),
            (
                "timestamp",
                json!({ "type": "string", "format": "date-time" }),
            ),
            ("bd_id", json!({ "type": ["string", "null"] })),
        ]),
        &[
            "ref",
            "source",
            "provider",
            "session_id",
            "turn",
            "kind",
            "snippet",
            "timestamp",
        ],
    )
}

pub(in crate::schema) fn decisions_array_schema() -> Value {
    json!({
        "type": "array",
        "description": "Top-scoring decision candidates, sorted by score desc.",
        "items": decision_candidate_item_schema()
    })
}

pub(in crate::schema) fn todos_array_schema() -> Value {
    json!({
        "type": "array",
        "description": "Open TODOs / follow-ups / bd refs, newest first.",
        "items": todo_candidate_item_schema()
    })
}
