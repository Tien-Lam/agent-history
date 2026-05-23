use chrono::{DateTime, Utc};

use crate::model::{ContentBlock, Message, MessageId, Role};
use crate::provider::parse_common::timestamp_value_to_utc;

use super::super::RawEntry;

pub(super) fn entry_timestamp(entry: &RawEntry) -> DateTime<Utc> {
    timestamp_value_to_utc(entry.timestamp.as_ref(), &["timestamp", "time", "value"])
        .unwrap_or_else(Utc::now)
}

pub(super) fn message(role: Role, timestamp: DateTime<Utc>, content: Vec<ContentBlock>) -> Message {
    Message {
        id: MessageId(String::new()),
        role,
        timestamp,
        content,
        model: None,
        token_usage: None,
    }
}

pub(super) fn assign_fallback_message_ids(messages: &mut [Message]) {
    for (idx, message) in messages.iter_mut().enumerate() {
        if message.id.0.is_empty() {
            message.id = MessageId(format!("codex-turn-{}", idx + 1));
        }
    }
}

pub(super) fn error_message(timestamp: DateTime<Utc>, error_msg: String) -> Message {
    message(
        Role::System,
        timestamp,
        vec![ContentBlock::Error(error_msg)],
    )
}
