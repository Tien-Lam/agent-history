use crate::model::{CitationRef, Message, Provider, SessionId};

use super::{TodoCandidate, TodoKind};

mod line;
mod text;

use line::scan_line;
use text::collect_text;

/// Maximum snippet length emitted per candidate. Keeps the JSON output
/// scannable when long lines (e.g. minified tool args) match a keyword.
pub(super) const SNIPPET_MAX: usize = 240;

/// Scan a session's messages and return every candidate matching `kinds`.
///
/// `kinds` empty means "all kinds". Order is deterministic: messages in
/// load order, lines within a message in document order, kinds in the
/// order they're scanned (TODO, follow-up, come-back-to, we-should, bd-ref).
pub fn extract_from_messages(
    provider: Provider,
    session_id: &SessionId,
    messages: &[Message],
    kinds: &[TodoKind],
) -> Vec<TodoCandidate> {
    let mut out = Vec::new();
    for (idx, msg) in messages.iter().enumerate() {
        let turn = u32::try_from(idx + 1).unwrap_or(u32::MAX);
        let Some(citation) = CitationRef::new(provider, session_id.clone(), turn) else {
            continue;
        };
        let text = collect_text(msg);
        for raw_line in text.lines() {
            scan_line(&citation, msg, raw_line, kinds, &mut out);
        }
    }
    out
}
