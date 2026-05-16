use aghist::provider;
use aghist::todos::{self, TodoCandidate, TodoKind};

use crate::cli::FilterArgs;
use crate::commands::filtering::{message_matches, session_matches};

use super::SessionMetaMap;

pub(super) fn collect_todo_candidates(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    kinds: &[TodoKind],
    record_session_meta: bool,
) -> (Vec<TodoCandidate>, SessionMetaMap) {
    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut candidates = Vec::new();
    let mut session_meta = SessionMetaMap::new();

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
            if !session_matches(&session, filters, project_needle.as_deref()) {
                continue;
            }
            let Ok(messages) = provider.load_messages(&session) else {
                continue;
            };
            let mut session_emitted = false;
            for candidate in
                todos::extract_from_messages(provider.provider(), &session.id, &messages, kinds)
            {
                if !candidate_matches_filters(&candidate, &messages, filters) {
                    continue;
                }
                if record_session_meta && !session_emitted {
                    session_meta.insert(
                        (provider.provider(), session.id.clone()),
                        (session.project_name.clone(), session.started_at),
                    );
                    session_emitted = true;
                }
                candidates.push(candidate);
            }
        }
    }

    (candidates, session_meta)
}

fn candidate_matches_filters(
    candidate: &TodoCandidate,
    messages: &[aghist::model::Message],
    filters: &FilterArgs,
) -> bool {
    if filters.role.is_some() || filters.has_tool_call {
        let turn_idx = (candidate.citation.turn as usize).saturating_sub(1);
        let Some(message) = messages.get(turn_idx) else {
            return false;
        };
        if !message_matches(message, filters) {
            return false;
        }
    }
    if let Some(since) = filters.since {
        if candidate.timestamp < since {
            return false;
        }
    }
    if let Some(until) = filters.until {
        if candidate.timestamp > until {
            return false;
        }
    }
    true
}
