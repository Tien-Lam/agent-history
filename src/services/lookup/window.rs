use crate::cli_error::ErrorEnvelope;
use crate::model::{CitationRef, Message, Session};

use super::LoadedCitationWindow;

pub(super) fn citation_window(
    session: Session,
    source: String,
    citation: CitationRef,
    citation_ref: String,
    messages: Vec<Message>,
    include_context: usize,
) -> Result<LoadedCitationWindow, ErrorEnvelope> {
    let total = messages.len();
    let turn = citation.turn as usize;
    if turn == 0 || turn > total {
        return Err(ErrorEnvelope::new(
            "session-not-found",
            format!("turn {turn} out of range: session has {total} message(s)"),
        )
        .with_hint("Use `aghist export` to inspect the full session, or pick a smaller turn."));
    }

    let target_idx = turn - 1;
    let start_idx = target_idx.saturating_sub(include_context);
    let end_idx = (target_idx + include_context + 1).min(total);
    let messages = messages
        .into_iter()
        .skip(start_idx)
        .take(end_idx - start_idx)
        .collect();
    Ok(LoadedCitationWindow {
        session,
        source,
        citation,
        citation_ref,
        messages,
        start_idx,
        target_idx,
        total_messages: total,
    })
}
