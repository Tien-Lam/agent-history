use aghist::model::Session;
use aghist::provider;

use crate::cli::FilterArgs;
use crate::commands::filtering::session_matches;

pub(super) fn collect_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
) -> Vec<Session> {
    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut sessions = Vec::new();
    for provider in providers {
        if let Some(want) = filters.provider {
            if provider.provider() != want {
                continue;
            }
        }
        match provider.discover_sessions() {
            Ok(found) => sessions.extend(
                found
                    .into_iter()
                    .filter(|session| session_matches(session, filters, project_needle.as_deref())),
            ),
            Err(e) => eprintln!("{}: error: {e}", provider.provider()),
        }
    }
    sessions
}
