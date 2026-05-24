use crate::model::Session;
use crate::provider::{self, HistoryProvider};
use crate::search::SearchFilters;

pub(super) fn session_has_matching_message(
    providers: &[Box<dyn HistoryProvider>],
    session: &Session,
    filters: &SearchFilters,
) -> Result<bool, String> {
    let messages = provider::load_messages_for_session(session, providers)
        .map_err(|error| error.to_string())?;
    Ok(messages
        .iter()
        .any(|message| filters.matches_message(message)))
}
