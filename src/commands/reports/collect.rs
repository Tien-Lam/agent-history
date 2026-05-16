use aghist::model::{Message, Session};
use aghist::provider;

use super::super::filtering::session_matches;
use crate::cli::FilterArgs;

pub(super) type SessionBundle = (Session, Vec<Message>);

pub(super) fn normalized_project_filter(filters: &FilterArgs) -> Option<String> {
    filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty())
}

pub(super) fn collect_filtered_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    project_needle: Option<&str>,
) -> Vec<Session> {
    let mut sessions = Vec::new();
    visit_matching_sessions(providers, filters, project_needle, |_, session| {
        sessions.push(session);
    });
    sessions
}

pub(super) fn collect_message_bundles(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    project_needle: Option<&str>,
    include_session: impl Fn(&Session) -> bool,
) -> Vec<SessionBundle> {
    let mut bundles = Vec::new();
    visit_matching_sessions(providers, filters, project_needle, |provider, session| {
        if !include_session(&session) {
            return;
        }
        let Ok(messages) = provider.load_messages(&session) else {
            return;
        };
        bundles.push((session, messages));
    });
    bundles
}

fn visit_matching_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    project_needle: Option<&str>,
    mut visit: impl FnMut(&dyn provider::HistoryProvider, Session),
) {
    for provider in providers {
        if let Some(want) = filters.provider {
            if provider.provider() != want {
                continue;
            }
        }
        let sessions = match provider.discover_sessions() {
            Ok(sessions) => sessions,
            Err(e) => {
                eprintln!("{}: error: {e}", provider.provider());
                continue;
            }
        };
        for session in sessions {
            if session_matches(&session, filters, project_needle) {
                visit(provider.as_ref(), session);
            }
        }
    }
}
