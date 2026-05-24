use serde_json::{json, Value};

use crate::schema_fragments::{USAGE_LIMIT_DEFAULT, USAGE_LIMIT_MAX};

use super::super::common::{
    closed_object_schema, exit_codes, schema_props_with_filters, SCHEMA_DRAFT,
};

fn usage_params_schema() -> Value {
    closed_object_schema(
        schema_props_with_filters([
            (
                "by",
                json!({
                    "type": "string",
                    "enum": ["model", "provider", "project"],
                    "default": "model",
                    "description": "Group rows by this dimension."
                }),
            ),
            (
                "limit",
                json!({
                    "type": "integer",
                    "minimum": 1,
                    "maximum": USAGE_LIMIT_MAX,
                    "default": USAGE_LIMIT_DEFAULT,
                    "description": "Cap rows after sorting. Totals always cover every matching session."
                }),
            ),
            (
                "json",
                json!({ "type": "boolean", "description": "Force JSON output (default: JSON on pipe, table on TTY)." }),
            ),
        ]),
        &[],
    )
}

fn usage_row_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "key": {
                "type": "string",
                "description": "Bucket key — model id, provider slug, or project name. Empty values become `(unknown)`."
            },
            "session_count": { "type": "integer", "minimum": 0 },
            "message_count": { "type": "integer", "minimum": 0 },
            "input_tokens": { "type": "integer", "minimum": 0 },
            "output_tokens": { "type": "integer", "minimum": 0 },
            "cache_read_tokens": { "type": "integer", "minimum": 0 },
            "cache_write_tokens": { "type": "integer", "minimum": 0 },
            "total_tokens": { "type": "integer", "minimum": 0 },
            "cost_usd": {
                "type": ["number", "null"],
                "description": "USD cost across the bucket, or null when any constituent session uses an unpriced model. We don't extrapolate prices."
            }
        },
        "required": ["key", "session_count", "message_count", "input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens", "total_tokens", "cost_usd"]
    })
}

fn usage_totals_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_count": { "type": "integer", "minimum": 0 },
            "message_count": { "type": "integer", "minimum": 0 },
            "input_tokens": { "type": "integer", "minimum": 0 },
            "output_tokens": { "type": "integer", "minimum": 0 },
            "cache_read_tokens": { "type": "integer", "minimum": 0 },
            "cache_write_tokens": { "type": "integer", "minimum": 0 },
            "total_tokens": { "type": "integer", "minimum": 0 },
            "cost_usd": { "type": ["number", "null"] }
        },
        "required": ["session_count", "message_count", "input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens", "total_tokens", "cost_usd"]
    })
}

fn usage_response_schema() -> Value {
    json!({
        "type": "object",
        "description": "JSON output (when --json or stdout is not a TTY).",
        "properties": {
            "rows": {
                "type": "array",
                "description": "Rows sorted by total_tokens descending, key ascending as tiebreaker.",
                "items": { "$ref": "#/definitions/UsageRow" }
            },
            "totals": { "$ref": "#/definitions/UsageTotals" },
            "meta": {
                "type": "object",
                "properties": {
                    "group_by": { "type": "string", "enum": ["model", "provider", "project"] },
                    "row_count": { "type": "integer", "minimum": 0 },
                    "total_row_count": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Total bucket count before --limit truncation."
                    }
                },
                "required": ["group_by", "row_count", "total_row_count"]
            }
        },
        "required": ["rows", "totals", "meta"]
    })
}

pub(in crate::schema) fn usage_schema() -> Value {
    json!({
        "$schema": SCHEMA_DRAFT,
        "$id": "aghist:schema/usage",
        "title": "aghist usage",
        "command": "usage",
        "description": "Aggregate token usage and (when pricing is known) USD cost across sessions. Pricing comes from a hand-curated table — unpriced sessions report cost_usd:null and null any total they roll into.",
        "params": usage_params_schema(),
        "response": usage_response_schema(),
        "definitions": {
            "UsageRow": usage_row_schema(),
            "UsageTotals": usage_totals_schema()
        },
        "exit_codes": exit_codes()
    })
}
