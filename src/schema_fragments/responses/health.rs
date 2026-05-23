use serde_json::{json, Value};

use super::super::common::{array_schema, closed_object_schema, schema_props};

pub(crate) fn health_response_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("ok", json!({ "type": "boolean" })),
            ("checks", array_schema(health_check_schema())),
            ("summary", health_summary_schema()),
            (
                "provider_fidelity",
                array_schema(provider_fidelity_item_schema()),
            ),
        ]),
        &["ok", "checks", "summary", "provider_fidelity"],
    )
}

fn health_check_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("name", json!({ "type": "string" })),
            (
                "status",
                json!({ "type": "string", "enum": ["ok", "warn", "fail"] }),
            ),
            ("message", json!({ "type": "string" })),
            ("hint", json!({ "type": ["string", "null"] })),
        ]),
        &["name", "status", "message"],
    )
}

fn health_summary_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("ok_count", json!({ "type": "integer", "minimum": 0 })),
            ("warn_count", json!({ "type": "integer", "minimum": 0 })),
            ("fail_count", json!({ "type": "integer", "minimum": 0 })),
        ]),
        &["ok_count", "warn_count", "fail_count"],
    )
}

fn provider_parse_stats_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("records_seen", json!({ "type": "integer", "minimum": 0 })),
            ("parse_errors", json!({ "type": "integer", "minimum": 0 })),
            (
                "skipped_records",
                json!({ "type": "integer", "minimum": 0 }),
            ),
            ("empty_content", json!({ "type": "integer", "minimum": 0 })),
        ]),
        &[
            "records_seen",
            "parse_errors",
            "skipped_records",
            "empty_content",
        ],
    )
}

fn provider_block_counts_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("text", json!({ "type": "integer", "minimum": 0 })),
            ("code_block", json!({ "type": "integer", "minimum": 0 })),
            ("tool_use", json!({ "type": "integer", "minimum": 0 })),
            ("tool_result", json!({ "type": "integer", "minimum": 0 })),
            ("thinking", json!({ "type": "integer", "minimum": 0 })),
            ("error", json!({ "type": "integer", "minimum": 0 })),
            ("total", json!({ "type": "integer", "minimum": 0 })),
        ]),
        &[
            "text",
            "code_block",
            "tool_use",
            "tool_result",
            "thinking",
            "error",
            "total",
        ],
    )
}

fn provider_tool_call_fidelity_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("tool_calls", json!({ "type": "integer", "minimum": 0 })),
            ("tool_results", json!({ "type": "integer", "minimum": 0 })),
            ("paired", json!({ "type": "integer", "minimum": 0 })),
            ("unpaired_calls", json!({ "type": "integer", "minimum": 0 })),
            ("orphan_results", json!({ "type": "integer", "minimum": 0 })),
            ("empty_names", json!({ "type": "integer", "minimum": 0 })),
            ("empty_call_ids", json!({ "type": "integer", "minimum": 0 })),
            (
                "empty_result_ids",
                json!({ "type": "integer", "minimum": 0 }),
            ),
            (
                "invalid_json_args",
                json!({ "type": "integer", "minimum": 0 }),
            ),
            (
                "success_results",
                json!({ "type": "integer", "minimum": 0 }),
            ),
            (
                "failure_results",
                json!({ "type": "integer", "minimum": 0 }),
            ),
        ]),
        &[
            "tool_calls",
            "tool_results",
            "paired",
            "unpaired_calls",
            "orphan_results",
            "empty_names",
            "empty_call_ids",
            "empty_result_ids",
            "invalid_json_args",
            "success_results",
            "failure_results",
        ],
    )
}

fn provider_fidelity_item_schema() -> Value {
    closed_object_schema(
        schema_props([
            ("label", json!({ "type": "string" })),
            ("provider", json!({ "type": "string" })),
            ("session_count", json!({ "type": "integer", "minimum": 0 })),
            ("message_count", json!({ "type": "integer", "minimum": 0 })),
            ("parse", provider_parse_stats_schema()),
            ("blocks", provider_block_counts_schema()),
            ("tool_call_fidelity", provider_tool_call_fidelity_schema()),
        ]),
        &[
            "label",
            "provider",
            "session_count",
            "message_count",
            "parse",
            "blocks",
            "tool_call_fidelity",
        ],
    )
}
