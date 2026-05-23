use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::provider::parse_common::timestamp_value_to_utc;

#[derive(Deserialize)]
pub(super) struct RawSessionEntry {
    #[serde(rename = "type")]
    pub(super) entry_type: Option<Value>,
    pub(super) uuid: Option<Value>,
    pub(super) timestamp: Option<Value>,
    pub(super) message: Option<RawMessage>,
    #[serde(rename = "gitBranch")]
    pub(super) git_branch: Option<Value>,
    pub(super) cwd: Option<Value>,
}

#[derive(Deserialize)]
pub(super) struct RawMessage {
    pub(super) content: Option<serde_json::Value>,
    pub(super) model: Option<Value>,
    pub(super) usage: Option<RawUsage>,
}

#[allow(clippy::struct_field_names)] // Provider JSON uses token-suffixed usage fields.
#[derive(Deserialize)]
pub(super) struct RawUsage {
    pub(super) input_tokens: Option<Value>,
    pub(super) output_tokens: Option<Value>,
    pub(super) cache_read_input_tokens: Option<Value>,
    pub(super) cache_creation_input_tokens: Option<Value>,
}

pub(super) fn claude_timestamp(value: Option<&Value>) -> Option<DateTime<Utc>> {
    timestamp_value_to_utc(value, &["timestamp", "createdAt", "value"])
}
