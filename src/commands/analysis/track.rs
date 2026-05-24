use std::collections::HashSet;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::{provider, query_scope};

use crate::cli::FilterArgs;

use super::common::map_llm_error;

mod output;
mod scan;

use output::emit_track_output;
use scan::scan_topic_sessions;

#[derive(Clone, Copy)]
pub(crate) struct TrackCommandRequest<'a> {
    pub(crate) filters: &'a FilterArgs,
    pub(crate) metadata_keys: Option<&'a HashSet<String>>,
    pub(crate) topic: &'a str,
    pub(crate) limit: usize,
    pub(crate) force_json: bool,
    pub(crate) llm_model: Option<&'a str>,
}

pub(crate) fn track_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    request: TrackCommandRequest<'_>,
) -> Result<i32, ErrorEnvelope> {
    let TrackCommandRequest {
        filters,
        metadata_keys,
        topic,
        limit,
        force_json,
        llm_model,
    } = request;
    let topic = topic.trim();
    if topic.is_empty() {
        return Err(ErrorEnvelope::new(
            "usage",
            "track <topic> must not be empty",
        ));
    }

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

    emit_track_output(topic, matched.len(), &events, force_json)?;
    Ok(EXIT_OK)
}
