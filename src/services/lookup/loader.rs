use crate::cli_error::ErrorEnvelope;
use crate::model::{Message, Session};
use crate::provider::{self, HistoryProvider};

pub(super) fn load_messages(
    providers: &[Box<dyn HistoryProvider>],
    session: &Session,
    display_ref: &str,
) -> Result<Vec<Message>, ErrorEnvelope> {
    provider::load_messages_for_session(session, providers).map_err(|e| {
        ErrorEnvelope::new(
            "provider-error",
            format!("failed to load messages for {display_ref}: {e}"),
        )
    })
}
