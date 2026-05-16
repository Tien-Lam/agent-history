use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::Provider;
use aghist::todos::TodoCandidate;
use chrono::{DateTime, Utc};

use super::super::common::map_llm_error;
use super::output::{render_llm_todos_human, render_llm_todos_json};
use super::{LlmTodoRow, SessionMetaMap};

/// Groups candidates by `(provider, session_id)` and issues one Messages
/// API call per session. The system prompt is cache-controlled so calls
/// 2..N pay near-zero on the static prompt tokens. Falls through to
/// `EXIT_EMPTY` when no todos survive.
pub(super) fn run_llm_todos(
    candidates: Vec<TodoCandidate>,
    session_meta: &SessionMetaMap,
    limit: usize,
    force_json: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if candidates.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let mut config = aghist::llm::LlmConfig::from_env().map_err(|e| map_llm_error(&e))?;
    if let Some(model) = llm_model {
        config = config.with_model(model.to_string());
    }
    let transport = aghist::llm::UreqTransport::new(config.timeout);

    let groups = group_by_session(candidates);
    let mut out = run_extraction(&transport, &config, groups, session_meta)?;

    out.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| a.citation.session_id.0.cmp(&b.citation.session_id.0))
            .then_with(|| a.citation.turn.cmp(&b.citation.turn))
    });
    if limit > 0 && out.len() > limit {
        out.truncate(limit);
    }
    if out.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut sink = stdout.lock();
    if want_json {
        render_llm_todos_json(&mut sink, &out)
    } else {
        render_llm_todos_human(&mut sink, &out)
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write todos output: {e}")))?;

    Ok(EXIT_OK)
}

fn group_by_session(
    candidates: Vec<TodoCandidate>,
) -> Vec<((Provider, aghist::model::SessionId), Vec<TodoCandidate>)> {
    let mut order: Vec<(Provider, aghist::model::SessionId)> = Vec::new();
    let mut grouped: std::collections::HashMap<
        (Provider, aghist::model::SessionId),
        Vec<TodoCandidate>,
    > = std::collections::HashMap::new();
    for candidate in candidates {
        let key = (
            candidate.citation.provider,
            candidate.citation.session_id.clone(),
        );
        if !grouped.contains_key(&key) {
            order.push(key.clone());
        }
        grouped.entry(key).or_default().push(candidate);
    }

    let mut out = Vec::with_capacity(order.len());
    for key in order {
        let group = grouped.remove(&key).unwrap_or_default();
        out.push((key, group));
    }
    out
}

fn run_extraction<T: aghist::llm::LlmTransport + ?Sized>(
    transport: &T,
    config: &aghist::llm::LlmConfig,
    groups: Vec<((Provider, aghist::model::SessionId), Vec<TodoCandidate>)>,
    session_meta: &SessionMetaMap,
) -> Result<Vec<LlmTodoRow>, ErrorEnvelope> {
    let mut out = Vec::new();
    for (key, group) in groups {
        let (project, started_at) = session_meta
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (None, DateTime::<Utc>::from_timestamp(0, 0).unwrap()));
        let llm_candidates: Vec<aghist::llm::TodoCandidate> = group
            .iter()
            .map(|candidate| aghist::llm::TodoCandidate {
                turn: candidate.citation.turn,
                role: candidate.role,
                kind: candidate.kind.slug(),
                snippet: candidate.snippet.as_str(),
            })
            .collect();
        let input = aghist::llm::TodoExtractionInput {
            provider: key.0,
            session_id: &key.1,
            project: project.as_deref(),
            candidates: llm_candidates,
        };
        let extracted = aghist::llm::extract_for_session_todos(transport, config, &input)
            .map_err(|e| map_llm_error(&e))?;
        for todo in extracted {
            out.push(LlmTodoRow {
                citation: todo.citation,
                todo: todo.todo,
                source_snippet: todo.source_snippet,
                source_kind: todo.source_kind,
                project: project.clone(),
                started_at,
            });
        }
    }
    Ok(out)
}
