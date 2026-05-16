use serde_json::{json, Value};

use super::super::common::{exit_codes, filter_params_fragment, SCHEMA_DRAFT};

fn usage_params_properties() -> Value {
    let mut props = serde_json::Map::new();
    props.insert(
        "by".to_string(),
        json!({
            "type": "string",
            "enum": ["model", "provider", "project"],
            "default": "model",
            "description": "Group rows by this dimension."
        }),
    );
    props.insert(
        "limit".to_string(),
        json!({
            "type": "integer",
            "minimum": 0,
            "default": 0,
            "description": "Cap rows after sorting (0 = no limit). Totals always cover every matching session."
        }),
    );
    props.insert(
        "json".to_string(),
        json!({ "type": "boolean", "description": "Force JSON output (default: JSON on pipe, table on TTY)." }),
    );
    for (name, schema) in filter_params_fragment() {
        props.insert(name.to_string(), schema);
    }
    Value::Object(props)
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
        "params": {
            "type": "object",
            "properties": usage_params_properties(),
            "additionalProperties": false
        },
        "response": usage_response_schema(),
        "definitions": {
            "UsageRow": usage_row_schema(),
            "UsageTotals": usage_totals_schema()
        },
        "exit_codes": exit_codes()
    })
}
