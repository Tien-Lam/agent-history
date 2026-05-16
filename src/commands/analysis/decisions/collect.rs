use aghist::model::Message;
use aghist::provider;

use crate::cli::FilterArgs;
use crate::commands::filtering::{message_matches, session_matches};

use super::DecisionRow;

/// Run the heuristic across all matching sessions, returning unsorted rows.
pub(super) fn collect_decision_rows(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    project_needle: Option<&str>,
    session_needle: Option<&str>,
    threshold: f32,
) -> Vec<DecisionRow> {
    let mut rows: Vec<DecisionRow> = Vec::new();
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
            if !session_matches(&session, filters, project_needle) {
                continue;
            }
            if let Some(needle) = session_needle {
                if !session.id.0.starts_with(needle) {
                    continue;
                }
            }
            let Ok(messages) = provider.load_messages(&session) else {
                continue;
            };
            let scored: Vec<(usize, &Message)> = messages
                .iter()
                .enumerate()
                .filter(|(_, message)| message_matches(message, filters))
                .collect();
            for (idx, msg) in scored {
                let turn = u32::try_from(idx + 1).unwrap_or(u32::MAX);
                let candidates = aghist::decisions::extract_from_message(msg, turn, threshold);
                for candidate in candidates {
                    let Some(citation) = aghist::model::CitationRef::new(
                        session.provider,
                        session.id.clone(),
                        candidate.turn,
                    ) else {
                        continue;
                    };
                    rows.push(DecisionRow {
                        citation,
                        candidate,
                        project: session.project_name.clone(),
                        started_at: session.started_at,
                    });
                }
            }
        }
    }
    rows
}
