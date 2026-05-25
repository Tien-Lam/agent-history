use std::collections::HashSet;

use aghist::model::ContentBlock;
use aghist::session_warnings::SessionLoadWarning;
use aghist::{provider, query_scope};

use crate::cli::FilterArgs;
use crate::commands::discovery::{federated_discovery_for_commands, source_for_session};
use crate::commands::filtering::{
    message_matches, metadata_filter_matches_source, session_matches,
};

/// Scan all providers for sessions mentioning `topic`, returning up to `limit` with excerpts.
pub(super) fn scan_topic_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
    topic: &str,
    limit: usize,
) -> Vec<aghist::llm::TrackSession> {
    let needle = topic.to_lowercase();
    let mut matched: Vec<aghist::llm::TrackSession> = Vec::new();
    let project_needle = filters.project_needle();

    let discovery = federated_discovery_for_commands(providers, scope);
    for session in discovery.sessions {
        if !session_matches(&session, filters, project_needle.as_deref()) {
            continue;
        }
        let source = source_for_session(&discovery.source_by_session, &session);
        if !metadata_filter_matches_source(&session, source, metadata_keys) {
            continue;
        }
        let messages = match provider::load_messages_for_session(&session, providers) {
            Ok(messages) => messages,
            Err(error) => {
                eprintln!(
                    "{}",
                    SessionLoadWarning::new(source, &session, error).warning_line()
                );
                continue;
            }
        };
        let mut excerpts: Vec<String> = Vec::new();
        for msg in &messages {
            if excerpts.len() >= 3 {
                break;
            }
            if !message_matches(msg, filters) {
                continue;
            }
            for block in &msg.content {
                if let ContentBlock::Text(t) = block {
                    if t.to_lowercase().contains(&needle) {
                        let snippet = t.trim();
                        let short = if snippet.chars().count() > 200 {
                            format!("{}…", snippet.chars().take(199).collect::<String>())
                        } else {
                            snippet.to_string()
                        };
                        excerpts.push(short);
                        break;
                    }
                }
            }
        }
        if excerpts.is_empty() {
            continue;
        }
        matched.push(aghist::llm::TrackSession {
            source: (source != aghist::federated::LOCAL_SOURCE).then(|| source.to_string()),
            provider: session.provider,
            session_id: session.id.clone(),
            started_at: session.started_at,
            excerpts,
        });
    }
    matched.sort_by_key(|s| s.started_at);
    if limit > 0 && matched.len() > limit {
        matched.truncate(limit);
    }
    matched
}
