use std::collections::HashSet;
use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::CitationRef;
use aghist::{provider, query_scope};
use chrono::{DateTime, Utc};

use crate::cli::FilterArgs;

use super::common::should_emit_json;

mod collect;
mod llm;
mod output;

use collect::{collect_federated_decision_rows, DecisionCollectRequest};
use llm::run_llm_decisions;
use output::{render_decisions_human, render_decisions_json};

#[derive(Clone, Copy)]
pub(crate) struct DecisionsCommandRequest<'a> {
    pub(crate) session_filter: Option<&'a str>,
    pub(crate) threshold: f32,
    pub(crate) limit: usize,
    pub(crate) force_json: bool,
    pub(crate) filters: &'a FilterArgs,
    pub(crate) metadata_keys: Option<&'a HashSet<String>>,
    pub(crate) use_llm: bool,
    pub(crate) llm_model: Option<&'a str>,
}

pub(crate) fn decisions_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    request: DecisionsCommandRequest<'_>,
) -> Result<i32, ErrorEnvelope> {
    let DecisionsCommandRequest {
        session_filter,
        threshold,
        limit,
        force_json,
        filters,
        metadata_keys,
        use_llm,
        llm_model,
    } = request;
    if !use_llm && llm_model.is_some() {
        return Err(ErrorEnvelope::new("usage", "--llm-model requires --llm"));
    }
    if !threshold.is_finite() || threshold < 0.0 {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("--threshold must be a non-negative finite number (got {threshold})"),
        ));
    }

    // If --session was given as a full citation ref, drop the trailing
    // `#turn` so it can match the session id; we extract decisions across
    // the whole session regardless of the cited turn.
    let session_needle = session_filter.map(|s| {
        let trimmed = s.trim();
        let without_turn = trimmed.rsplit_once('#').map_or(trimmed, |(head, _)| head);
        // Strip leading provider segment if present (`<slug>/<id>` → `<id>`).
        without_turn
            .split_once('/')
            .map_or(without_turn, |(_, rest)| rest)
            .to_string()
    });
    let source_needle = session_filter.and_then(|s| {
        let trimmed = s.trim();
        let without_turn = trimmed.rsplit_once('#').map_or(trimmed, |(head, _)| head);
        let slash = without_turn.find('/');
        let colon = without_turn.find(':');
        match (colon, slash) {
            (Some(c), Some(s)) if c < s => Some(without_turn[..c].to_string()),
            _ => None,
        }
    });

    let mut rows = collect_federated_decision_rows(
        providers,
        scope,
        DecisionCollectRequest {
            filters,
            session_needle: session_needle.as_deref(),
            source_needle: source_needle.as_deref(),
            metadata_keys,
            threshold,
        },
    );

    rows.sort_by(|a, b| {
        b.candidate
            .score
            .partial_cmp(&a.candidate.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.started_at.cmp(&a.started_at))
            .then_with(|| a.citation.session_id.0.cmp(&b.citation.session_id.0))
            .then_with(|| a.candidate.turn.cmp(&b.candidate.turn))
    });

    if rows.len() > limit {
        rows.truncate(limit);
    }

    if use_llm {
        return run_llm_decisions(rows, limit, force_json, llm_model);
    }

    if rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = should_emit_json(force_json);
    let stdout = io::stdout();
    let mut sink = stdout.lock();
    if want_json {
        render_decisions_json(&mut sink, &rows)
    } else {
        render_decisions_human(&mut sink, &rows)
    }
    .map_err(|e| ErrorEnvelope::io("failed to write decisions output", e))?;

    Ok(EXIT_OK)
}

struct LlmRow {
    citation: CitationRef,
    source: String,
    decision: aghist::llm::StructuredDecision,
    source_snippet: Option<String>,
    project: Option<String>,
    started_at: DateTime<Utc>,
}

impl LlmRow {
    fn reference(&self) -> String {
        if self.source == aghist::federated::LOCAL_SOURCE {
            self.citation.to_string()
        } else {
            format!("{}:{}", self.source, self.citation)
        }
    }
}

struct DecisionRow {
    citation: aghist::model::CitationRef,
    candidate: aghist::decisions::DecisionCandidate,
    source: String,
    project: Option<String>,
    started_at: DateTime<Utc>,
}

impl DecisionRow {
    fn reference(&self) -> String {
        if self.source == aghist::federated::LOCAL_SOURCE {
            self.citation.to_string()
        } else {
            format!("{}:{}", self.source, self.citation)
        }
    }
}
