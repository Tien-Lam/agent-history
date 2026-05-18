use std::path::Path;

use chrono::Utc;

use super::super::ProviderError;
use crate::model::{Message, MessageId, Role};
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(crate) fn parse_checkpoint_md(path: &Path) -> Result<Vec<Message>, ProviderError> {
    let content = std::fs::read_to_string(path)?;
    if content.trim().is_empty()
        || content
            .lines()
            .all(|l| l.starts_with('#') || l.starts_with('|') || l.trim().is_empty())
    {
        return Ok(Vec::new());
    }

    Ok(vec![Message {
        id: MessageId("checkpoint".to_string()),
        role: Role::System,
        timestamp: Utc::now(),
        content: parse_text_with_code_blocks(&content),
        model: None,
        token_usage: None,
    }])
}
