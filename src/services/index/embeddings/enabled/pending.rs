use std::collections::HashSet;

use crate::embed;
use crate::model::{ContentBlock, Message, Session};

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
    let parts: Vec<&str> = message
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text(t) | ContentBlock::Thinking(t) | ContentBlock::Error(t) => {
                t.as_str()
            }
            ContentBlock::CodeBlock { code, .. } => code.as_str(),
            ContentBlock::ToolUse(tc) => tc.arguments.as_str(),
            ContentBlock::ToolResult(tr) => tr.output.as_str(),
        })
        .collect();
    parts.join("\n")
}
