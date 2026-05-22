use serde_json::{json, Value};

use crate::schema_fragments::index_response_schema;

use super::super::common::{
    closed_object_schema, exit_codes, provider_slug_enum, schema_props, SCHEMA_DRAFT,
};

fn index_params_schema() -> Value {
    closed_object_schema(
        schema_props([
            (
                "provider",
                json!({ "type": "string", "enum": provider_slug_enum() }),
            ),
            (
                "force",
                json!({ "type": "boolean", "default": false, "description": "Clear the index before rebuilding." }),
            ),
            (
                "accept_download",
                json!({ "type": "boolean", "default": false, "description": "Authorise the embedding-model download (~90 MB)." }),
            ),
        ]),
        &[],
    )
}

pub(in crate::schema) fn index_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/index",
        "title": "aghist index",
        "command": "index",
        "description": "Build or refresh the search index. Idempotent and delta-aware.",
        "params": index_params_schema(),
        "response": index_response_schema(),
        "exit_codes": exit_codes()
    })
}
