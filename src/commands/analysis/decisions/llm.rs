use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::{Provider, Role};
use chrono::{DateTime, Utc};

use super::super::common::{
    llm_config_from_env, map_llm_error, ordered_session_groups, should_emit_json,
};
use super::output::{render_llm_decisions_human, render_llm_decisions_json};
use super::{DecisionRow, LlmRow};

/// Route heuristic candidates through the LLM and emit structured decisions.
///
/// Groups rows by `(provider, session_id)` and issues one Messages API call
/// per session. The system prompt is cache-controlled, so calls 2..N pay
/// near-zero on the static prompt tokens. Falls through to `EXIT_EMPTY` if
/// no decisions survive.
pub(super) fn run_llm_decisions(
    rows: Vec<DecisionRow>,
    limit: usize,
    force_json: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let config = llm_config_from_env(llm_model)?;
    let transport = aghist::llm::UreqTransport::new(config.timeout);

    let groups = group_by_session(rows);
    let mut out = run_extraction(&transport, &config, groups)?;

    out.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| a.citation.session_id.0.cmp(&b.citation.session_id.0))
            .then_with(|| a.citation.turn.cmp(&b.citation.turn))
    });
    if out.len() > limit {
        out.truncate(limit);
    }
    if out.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = should_emit_json(force_json);
    let stdout = io::stdout();
    let mut sink = stdout.lock();
    if want_json {
        render_llm_decisions_json(&mut sink, &out)
    } else {
        render_llm_decisions_human(&mut sink, &out)
    }
    .map_err(|e| ErrorEnvelope::io("failed to write decisions output", e))?;

    Ok(EXIT_OK)
}

/// Group heuristic rows by `(provider, session_id)`, preserving first-seen
/// order so the API call sequence stays predictable.
fn group_by_session(rows: Vec<DecisionRow>) -> Vec<SessionGroup> {
    ordered_session_groups(
        rows,
        |row| {
            (
                row.source.clone(),
                row.citation.provider,
                row.citation.session_id.clone(),
            )
        },
        |row| SessionGroup {
            source: row.source.clone(),
            provider: row.citation.provider,
            session_id: row.citation.session_id.clone(),
            project: row.project.clone(),
            started_at: row.started_at,
            candidates: Vec::new(),
        },
        |group, row| {
            group.candidates.push(GroupedCandidate {
                turn: row.candidate.turn,
                role: row.candidate.role,
                snippet: row.candidate.snippet,
            });
        },
    )
}

fn run_extraction<T: aghist::llm::LlmTransport + ?Sized>(
    transport: &T,
    config: &aghist::llm::LlmConfig,
    groups: Vec<SessionGroup>,
) -> Result<Vec<LlmRow>, ErrorEnvelope> {
    let mut out = Vec::new();
    for group in groups {
        let candidates: Vec<aghist::llm::Candidate> = group
            .candidates
            .iter()
            .map(|candidate| aghist::llm::Candidate {
                turn: candidate.turn,
                role: candidate.role,
                snippet: candidate.snippet.as_str(),
            })
            .collect();
        let input = aghist::llm::ExtractionInput {
            provider: group.provider,
            session_id: &group.session_id,
            project: group.project.as_deref(),
            candidates,
        };
        let extracted = aghist::llm::extract_for_session(transport, config, &input)
            .map_err(|e| map_llm_error(&e))?;
        for decision in extracted {
            out.push(LlmRow {
                citation: decision.citation,
                source: group.source.clone(),
                decision: decision.decision,
                source_snippet: decision.source_snippet,
                project: group.project.clone(),
                started_at: group.started_at,
            });
        }
    }
    Ok(out)
}

struct SessionGroup {
    source: String,
    provider: Provider,
    session_id: aghist::model::SessionId,
    project: Option<String>,
    started_at: DateTime<Utc>,
    candidates: Vec<GroupedCandidate>,
}

struct GroupedCandidate {
    turn: u32,
    role: Role,
    snippet: String,
}
