use std::path::Path;

use super::super::ProviderError;
use crate::fs_read;
use crate::model::{Message, MessageId, Role};
use crate::provider::parse_common::{
    file_modified_utc, unix_epoch_utc, MAX_PROVIDER_SESSION_FILE_BYTES,
};
use crate::provider::text_blocks::parse_text_with_code_blocks;

pub(crate) fn parse_checkpoint_md(path: &Path) -> Result<Vec<Message>, ProviderError> {
    let content =
        fs_read::read_regular_file_to_string_limited(path, MAX_PROVIDER_SESSION_FILE_BYTES)?;
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
        timestamp: file_modified_utc(path).unwrap_or_else(unix_epoch_utc),
        content: parse_text_with_code_blocks(&content),
        model: None,
        token_usage: None,
    }])
}
