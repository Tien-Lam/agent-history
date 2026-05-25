use serde_json::{json, Value};

use crate::schema_fragments::{
    CLI_PATH_MAX_BYTES, EXPORT_TURN_RANGE_MAX_BYTES, REFERENCE_MAX_BYTES,
};

use super::super::super::common::{closed_object_schema, exit_codes, schema_props, SCHEMA_DRAFT};

pub(in crate::schema) fn export_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/export",
        "title": "aghist export",
        "command": "export",
        "description": "Export a session to Markdown, JSON, or HTML.",
        "params": closed_object_schema(
            schema_props([
                ("format", json!({ "type": "string", "enum": ["md", "json", "html"] })),
                (
                    "session",
                    json!({ "type": "string", "maxLength": REFERENCE_MAX_BYTES, "description": "Session ID/prefix, `<provider>/<session-id>`, or `<source>:<provider>/<session-id>`." }),
                ),
                (
                    "output",
                    json!({ "type": "string", "maxLength": CLI_PATH_MAX_BYTES, "description": "Output file path (defaults to stdout)." }),
                ),
                (
                    "turn_range",
                    json!({
                        "type": "string",
                        "maxLength": EXPORT_TURN_RANGE_MAX_BYTES,
                        "pattern": "^[0-9]*(:[0-9]*)?$",
                        "description": "1-based inclusive turn range: A:B, :B, A:, or a single A."
                    }),
                ),
                (
                    "include_notes",
                    json!({
                        "type": "boolean",
                        "description": "Inline private annotations (from the metadata sidecar) at their citation refs. Notes stay marked 'private annotation' in the output."
                    }),
                ),
            ]),
            &["format", "session"],
        ),
        "response": {
            "description": "Raw exported content written to stdout or `output` path. Format depends on `format` param."
        },
        "exit_codes": exit_codes()
    })
}
