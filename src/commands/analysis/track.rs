use std::collections::HashSet;
use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::ContentBlock;
use aghist::output::write_json_line;
use aghist::{provider, query_scope};

use crate::cli::FilterArgs;
use crate::commands::discovery::{federated_discovery_for_commands, source_for_session};
use crate::commands::filtering::{
    message_matches, metadata_filter_matches_source, session_matches,
};
use crate::commands::text::truncate;

use super::common::map_llm_error;

// ── track command ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub(crate) struct TrackCommandRequest<'a> {
    pub(crate) filters: &'a FilterArgs,
    pub(crate) metadata_keys: Option<&'a HashSet<String>>,
    pub(crate) topic: &'a str,
    pub(crate) limit: usize,
    pub(crate) force_json: bool,
    pub(crate) llm_model: Option<&'a str>,
}

/// Scan all providers for sessions mentioning `topic`, returning up to `limit` with excerpts.
fn scan_topic_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    filters: &FilterArgs,
    metadata_keys: Option<&HashSet<String>>,
    topic: &str,
    limit: usize,
) -> Vec<aghist::llm::TrackSession> {
    let needle = topic.to_lowercase();
    let mut matched: Vec<aghist::llm::TrackSession> = Vec::new();
    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let discovery = federated_discovery_for_commands(providers, scope);
    for session in discovery.sessions {
        if !session_matches(&session, filters, project_needle.as_deref()) {
            continue;
        }
        let source = source_for_session(&discovery.source_by_session, &session);
        if !metadata_filter_matches_source(&session, source, metadata_keys) {
            continue;
        }
        let Ok(messages) = provider::load_messages_for_session(&session, providers) else {
            continue;
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

pub(crate) fn track_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    request: TrackCommandRequest<'_>,
) -> Result<i32, ErrorEnvelope> {
    use std::io::Write as _;
    let TrackCommandRequest {
        filters,
        metadata_keys,
        topic,
        limit,
        force_json,
        llm_model,
    } = request;
    let matched = scan_topic_sessions(providers, scope, filters, metadata_keys, topic, limit);

    if matched.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let mut config = aghist::llm::LlmConfig::from_env().map_err(|e| map_llm_error(&e))?;
    if let Some(model) = llm_model {
        config = config.with_model(model.to_string());
    }
    let transport = aghist::llm::UreqTransport::new(config.timeout);

    let events = aghist::llm::extract_track(&transport, &config, topic, &matched)
        .map_err(|e| map_llm_error(&e))?;

    if events.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    if want_json {
        let payload = serde_json::json!({
            "topic": topic,
            "sessions_scanned": matched.len(),
            "timeline": events,
        });
        let stdout = io::stdout();
        let mut out = stdout.lock();
        write_json_line(&mut out, &payload)
            .map_err(|e| ErrorEnvelope::io("failed to write track output", e))?;
    } else {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        writeln!(out, "Topic: {topic}")
            .map_err(|e| ErrorEnvelope::io("failed to write track output", e))?;
        writeln!(out, "Sessions scanned: {}", matched.len())
            .map_err(|e| ErrorEnvelope::io("failed to write track output", e))?;
        writeln!(out).map_err(|e| ErrorEnvelope::io("failed to write track output", e))?;
        writeln!(
            out,
            "{:<10}  {:<42}  {:<12}  EVENT",
            "DATE", "REF", "DIRECTION"
        )
        .map_err(|e| ErrorEnvelope::io("failed to write track output", e))?;
        for ev in &events {
            let ref_short = truncate(&ev.session_ref, 42);
            let event_short = truncate(&ev.event, 80);
            writeln!(
                out,
                "{:<10}  {:<42}  {:<12}  {}",
                ev.date, ref_short, ev.direction, event_short
            )
            .map_err(|e| ErrorEnvelope::io("failed to write track output", e))?;
        }
        writeln!(out).map_err(|e| ErrorEnvelope::io("failed to write track output", e))?;
        writeln!(out, "Total: {} event(s)", events.len())
            .map_err(|e| ErrorEnvelope::io("failed to write track output", e))?;
    }

    Ok(EXIT_OK)
}
