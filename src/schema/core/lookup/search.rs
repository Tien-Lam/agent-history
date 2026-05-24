use serde_json::{json, Value};

use crate::schema_fragments::{
    SEARCH_HYBRID_WEIGHT_DEFAULT, SEARCH_LIMIT_DEFAULT, SEARCH_LIMIT_MAX,
    SEARCH_WATCH_INTERVAL_MS_DEFAULT, SEARCH_WATCH_ITERATIONS_DEFAULT,
};

use super::super::super::common::{
    closed_object_schema, exit_codes, filter_params_fragment, schema_props, search_response_schema,
    SchemaProperties, SCHEMA_DRAFT,
};

fn search_params_properties() -> SchemaProperties {
    let mut props = schema_props([
        (
            "query",
            json!({ "type": "string", "description": "Tantivy query string. Mutually exclusive with query_file/stdin." }),
        ),
        (
            "query_file",
            json!({ "type": "string", "description": "Read query from file path (use '-' for stdin)." }),
        ),
        (
            "stdin",
            json!({ "type": "boolean", "description": "Read query from standard input." }),
        ),
        (
            "limit",
            json!({ "type": "integer", "minimum": 1, "maximum": SEARCH_LIMIT_MAX, "default": SEARCH_LIMIT_DEFAULT }),
        ),
        (
            "cursor",
            json!({ "type": "string", "description": "Opaque pagination cursor from a prior `meta.next_cursor`." }),
        ),
        (
            "json",
            json!({ "type": "boolean", "description": "Force JSON output (default: JSON on pipe, table on TTY)." }),
        ),
        (
            "debug_search",
            json!({ "type": "boolean", "description": "Include Tantivy score explanation trees on each hit." }),
        ),
        (
            "watch",
            json!({ "type": "boolean", "description": "Long-running NDJSON stream of new hits." }),
        ),
        (
            "watch_interval_ms",
            json!({ "type": "integer", "minimum": 1, "default": SEARCH_WATCH_INTERVAL_MS_DEFAULT }),
        ),
        (
            "watch_iterations",
            json!({ "type": "integer", "minimum": 0, "default": SEARCH_WATCH_ITERATIONS_DEFAULT, "description": "Stop after N polls (0 = run until interrupted)." }),
        ),
        (
            "hybrid_weight",
            json!({
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
                "default": SEARCH_HYBRID_WEIGHT_DEFAULT,
                "description": "RRF weight on the semantic side. 0.0 = lexical only (default), 1.0 = semantic only. Fails open to lexical when embeddings unavailable."
            }),
        ),
    ]);
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }
    props
}

pub(in crate::schema) fn search_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/search",
        "title": "aghist search",
        "command": "search",
        "description": "Full-text search across indexed sessions. Returns hits with citation refs.",
        "params": closed_object_schema(search_params_properties(), &[]),
        "response": search_response_schema(),
        "exit_codes": exit_codes()
    })
}
