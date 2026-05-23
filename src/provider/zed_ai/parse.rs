use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::provider::parse_common::timestamp_value_to_utc;

mod messages;
mod session;

pub(crate) use messages::load_messages_from_path_with_stats;
pub(crate) use session::read_session;

#[derive(Debug, Deserialize)]
struct ZedConversation {
    id: Option<Value>,
    summary: Option<Value>,
    model: Option<Value>,
    workspace: Option<Value>,
    #[serde(default, alias = "createdAt")]
    created_at: Option<Value>,
    #[serde(default, alias = "updatedAt")]
    updated_at: Option<Value>,
    #[serde(default)]
    messages: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct ZedMessage {
    id: Option<Value>,
    role: Option<Value>,
    #[serde(default, alias = "content")]
    text: Option<Value>,
    #[serde(default, alias = "createdAt")]
    timestamp: Option<Value>,
    model: Option<Value>,
}

fn zed_timestamp(value: Option<&Value>) -> Option<DateTime<Utc>> {
    timestamp_value_to_utc(
        value,
        &[
            "timestamp",
            "time",
            "createdAt",
            "created_at",
            "updatedAt",
            "updated_at",
            "value",
        ],
    )
}
