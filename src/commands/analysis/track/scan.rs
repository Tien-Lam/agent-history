use std::collections::HashSet;

use aghist::model::ContentBlock;
use aghist::{provider, query_scope};

use crate::cli::FilterArgs;
use crate::commands::filtering::{
    collect_filtered_federated_sessions, load_messages_or_warn, PreparedFilters,
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
    let prepared_filters = PreparedFilters::from_args(filters);

    let filtered = collect_filtered_federated_sessions(providers, scope, filters, metadata_keys);
    for filtered_session in filtered.sessions {
        let source = filtered_session.source;
        let session = filtered_session.session;
        let Some(messages) = load_messages_or_warn(providers, &source, &session) else {
            continue;
        };
        let mut excerpts: Vec<String> = Vec::new();
        for msg in &messages {
            if excerpts.len() >= 3 {
                break;
            }
            if !prepared_filters.matches_message(msg) {
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
            source: (source != aghist::federated::LOCAL_SOURCE).then_some(source),
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
