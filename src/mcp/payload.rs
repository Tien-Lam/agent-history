use serde_json::{json, Value};

use crate::dto::{McpSessionRow, MessageRow};
use crate::federated::LOCAL_SOURCE;
use crate::model::{Message, QualifiedCitationRef, Session};
use crate::schema_fragments;

use super::resources::{session_uri_for_source, turn_uri_for_source};

pub(super) fn tool_definitions() -> Value {
    Value::Array(
        schema_fragments::mcp_tool_contracts()
            .into_iter()
            .map(|contract| {
                let mut tool = serde_json::Map::new();
                tool.insert("name".to_string(), json!(contract.name));
                tool.insert("description".to_string(), json!(contract.description));
                tool.insert("inputSchema".to_string(), contract.input_schema);
                if let Some(output_schema) = contract.output_schema {
                    tool.insert("outputSchema".to_string(), output_schema);
                }
                Value::Object(tool)
            })
            .collect(),
    )
}

pub(super) fn tool_success(payload: &Value) -> Value {
    let text =
        serde_json::to_string_pretty(payload).unwrap_or_else(|_| "<unserializable>".to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "structuredContent": payload,
    })
}

pub(super) fn tool_error(message: impl Into<String>) -> Value {
    let msg = message.into();
    json!({
        "content": [{ "type": "text", "text": msg }],
        "isError": true,
    })
}

pub(super) fn session_row_with_source(s: &Session, source: &str) -> Value {
    let row = McpSessionRow::from_session(
        s,
        source,
        session_uri_for_source(source, s.provider, &s.id.0),
    );
    serde_json::to_value(row).unwrap_or(Value::Null)
}

pub(super) fn message_row_with_source(
    session: &Session,
    msg: &Message,
    turn: usize,
    source: &str,
) -> Value {
    let turn_u32 = u32::try_from(turn).unwrap_or(u32::MAX);
    let ref_ = session.citation_ref(turn_u32).map(|citation| {
        QualifiedCitationRef::new(
            (source != LOCAL_SOURCE).then(|| source.to_string()),
            citation,
        )
        .to_string()
    });
    let row = MessageRow::from_message(
        msg,
        source,
        turn,
        ref_,
        turn_uri_for_source(source, session.provider, &session.id.0, turn_u32),
    );
    serde_json::to_value(row).unwrap_or(Value::Null)
}
