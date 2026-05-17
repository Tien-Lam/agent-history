use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::model::{ContentBlock, Message, Role};

#[derive(Debug, Clone, Serialize)]
pub struct MessageRow {
    #[serde(rename = "ref")]
    pub ref_: Option<String>,
    pub uri: String,
    pub source: String,
    pub turn: usize,
    pub id: String,
    pub role: Role,
    pub timestamp: DateTime<Utc>,
    pub model: Option<String>,
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_target: Option<bool>,
}

impl MessageRow {
    pub fn from_message(
        message: &Message,
        source: &str,
        turn: usize,
        ref_: Option<String>,
        uri: impl Into<String>,
    ) -> Self {
        Self {
            ref_,
            uri: uri.into(),
            source: source.to_string(),
            turn,
            id: message.id.0.clone(),
            role: message.role,
            timestamp: message.timestamp,
            model: message.model.clone(),
            content: message.content.clone(),
            is_target: None,
        }
    }

    #[must_use]
    pub fn with_target(mut self, is_target: bool) -> Self {
        self.is_target = Some(is_target);
        self
    }
}
