use serde_json::{json, Value};

use super::super::common::{
    array_schema, closed_object_schema, provider_slug_enum_nullable, schema_props,
    source_qualified_citation_ref_pattern, source_qualified_session_ref_pattern, with_description,
    SchemaProperties,
};
use super::list::source_error_schema;

fn search_meta_schema() -> Value {
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

pub(crate) fn search_hit_schema() -> Value {
    json!({
        "oneOf": [
            search_message_hit_schema(),
            search_note_hit_schema()
        ]
    })
}

fn search_hit_base_properties(kind: &'static str) -> SchemaProperties {
    schema_props([
        (
            "kind",
            json!({
                "type": "string",
                "enum": [kind],
                "description": "Whether the hit points at a session message or a metadata note."
            }),
        ),
        ("session_id", json!({ "type": "string" })),
        ("message_id", json!({ "type": "string" })),
        ("score", json!({ "type": "number" })),
        ("snippet", json!({ "type": "string" })),
        (
            "source",
            json!({
                "type": "string",
                "description": "`local` for this host, or a registered remote source name."
            }),
        ),
        (
            "explanation",
            json!({
                "type": "object",
                "description": "Present only with --debug-search; Tantivy score explanation tree."
            }),
        ),
    ])
}

fn search_message_hit_schema() -> Value {
    let mut properties = search_hit_base_properties("message");
    properties.extend(schema_props([
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
            "ref",
            json!({
                "type": "string",
                "pattern": source_qualified_citation_ref_pattern(),
                "description": "Citation ref for message hits."
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
    ]));

    closed_object_schema(
        properties,
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

fn search_note_hit_schema() -> Value {
    let mut properties = search_hit_base_properties("note");
    properties.extend(schema_props([
        ("provider", json!({ "type": "null" })),
        ("project", json!({ "type": "null" })),
        ("started_at", json!({ "type": "null" })),
        (
            "note_id",
            json!({
                "type": "integer",
                "description": "Metadata note row id."
            }),
        ),
        (
            "ref",
            json!({
                "type": "string",
                "pattern": source_qualified_session_ref_pattern(),
                "description": "The note's stored session ref."
            }),
        ),
    ]));

    closed_object_schema(
        properties,
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
            "note_id",
            "ref",
        ],
    )
}

pub(crate) fn search_response_schema() -> Value {
    let schema = closed_object_schema(
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
    with_description(
        schema,
        "JSON envelope emitted by `aghist search --json`; watch mode emits one hit object per NDJSON line.",
    )
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
