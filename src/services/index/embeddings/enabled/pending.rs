use std::collections::HashSet;

use crate::embed;
use crate::model::{ContentBlock, Message, Session};

const MAX_EMBEDDING_TEXT_BYTES: usize = 64 * 1024;

pub(super) type PendingEmbedding = (String, String, [u8; embed::HASH_LEN]);

pub(super) fn pending_embeddings(
    session: &Session,
    messages: &[Message],
    store: &embed::EmbeddingStore,
    live_keys: &mut HashSet<String>,
    messages_reused: &mut usize,
) -> Vec<PendingEmbedding> {
    // (id, text, content_hash) for messages whose cached vector is stale or
    // absent. We compute the hash up front so the freshness check is a cheap
    // byte compare against what's in the store.
    messages
        .iter()
        .enumerate()
        .filter_map(|(turn_index, m)| {
            let text = collect_text(m);
            if text.trim().is_empty() {
                return None;
            }
            let hash = embed::content_hash(&text);
            let message_key = session.message_key(turn_index, &m.id.0);
            live_keys.insert(message_key.clone());
            if store.get_if_fresh(&message_key, &hash).is_some() {
                *messages_reused += 1;
                return None;
            }
            Some((message_key, text, hash))
        })
        .collect()
}

fn collect_text(message: &Message) -> String {
    let mut out = String::new();
    for block in &message.content {
        if out.len() >= MAX_EMBEDDING_TEXT_BYTES {
            break;
        }
        if !out.is_empty() {
            push_bounded(&mut out, "\n");
        }
        push_bounded(&mut out, block_text(block));
    }
    out
}

fn block_text(block: &ContentBlock) -> &str {
    match block {
        ContentBlock::Text(t) | ContentBlock::Thinking(t) | ContentBlock::Error(t) => t.as_str(),
        ContentBlock::CodeBlock { code, .. } => code.as_str(),
        ContentBlock::ToolUse(tc) => tc.arguments.as_str(),
        ContentBlock::ToolResult(tr) => tr.output.as_str(),
    }
}

fn push_bounded(out: &mut String, text: &str) {
    let remaining = MAX_EMBEDDING_TEXT_BYTES.saturating_sub(out.len());
    if remaining == 0 {
        return;
    }
    if text.len() <= remaining {
        out.push_str(text);
        return;
    }
    let end = floor_char_boundary(text, remaining);
    out.push_str(&text[..end]);
}

fn floor_char_boundary(text: &str, limit: usize) -> usize {
    if limit >= text.len() {
        return text.len();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;
    use crate::model::{MessageId, Role};

    fn message(content: Vec<ContentBlock>) -> Message {
        Message {
            id: MessageId("msg".to_string()),
            role: Role::Assistant,
            timestamp: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            content,
            model: None,
            token_usage: None,
        }
    }

    #[test]
    fn collect_text_caps_embedding_input() {
        let text = "x".repeat(MAX_EMBEDDING_TEXT_BYTES + 10);
        let collected = collect_text(&message(vec![
            ContentBlock::Text(text),
            ContentBlock::Text("unreachable".to_string()),
        ]));

        assert_eq!(collected.len(), MAX_EMBEDDING_TEXT_BYTES);
        assert!(!collected.contains("unreachable"));
    }

    #[test]
    fn collect_text_truncates_at_utf8_boundary() {
        let text = "é".repeat((MAX_EMBEDDING_TEXT_BYTES / "é".len()) + 1);
        let collected = collect_text(&message(vec![ContentBlock::Text(text)]));

        assert!(collected.len() <= MAX_EMBEDDING_TEXT_BYTES);
        assert!(collected.ends_with('é'));
    }
}
