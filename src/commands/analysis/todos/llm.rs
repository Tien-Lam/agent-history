use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::Provider;
use aghist::todos::TodoCandidate;

use super::super::common::map_llm_error;
use super::output::{render_llm_todos_human, render_llm_todos_json};
use super::{LlmTodoRow, TodoRow};

/// Groups candidates by `(provider, session_id)` and issues one Messages
/// API call per session. The system prompt is cache-controlled so calls
/// 2..N pay near-zero on the static prompt tokens. Falls through to
/// `EXIT_EMPTY` when no todos survive.
pub(super) fn run_llm_todos(
    rows: Vec<TodoRow>,
    limit: usize,
    force_json: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let mut config = aghist::llm::LlmConfig::from_env().map_err(|e| map_llm_error(&e))?;
    if let Some(model) = llm_model {
        config = config.with_model(model.to_string());
    }
    let transport = aghist::llm::UreqTransport::new(config.timeout);

    let groups = group_by_session(rows);
    let mut out = run_extraction(&transport, &config, groups)?;

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

fn group_by_session(rows: Vec<TodoRow>) -> Vec<SessionGroup> {
    let mut order: Vec<(String, Provider, aghist::model::SessionId)> = Vec::new();
    let mut grouped: std::collections::HashMap<
        (String, Provider, aghist::model::SessionId),
        SessionGroup,
    > = std::collections::HashMap::new();
    for row in rows {
        let key = (
            row.source.clone(),
            row.candidate.citation.provider,
            row.candidate.citation.session_id.clone(),
        );
        let entry = grouped.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            SessionGroup {
                source: row.source.clone(),
                provider: row.candidate.citation.provider,
                session_id: row.candidate.citation.session_id.clone(),
                project: row.project.clone(),
                started_at: row.started_at,
                candidates: Vec::new(),
            }
        });
        entry.candidates.push(row.candidate);
    }

    let mut out = Vec::with_capacity(order.len());
    for key in order {
        if let Some(group) = grouped.remove(&key) {
            out.push(group);
        }
    }
    out
}

fn run_extraction<T: aghist::llm::LlmTransport + ?Sized>(
    transport: &T,
    config: &aghist::llm::LlmConfig,
    groups: Vec<SessionGroup>,
) -> Result<Vec<LlmTodoRow>, ErrorEnvelope> {
    let mut out = Vec::new();
    for group in groups {
        let llm_candidates: Vec<aghist::llm::TodoCandidate> = group
            .candidates
            .iter()
            .map(|candidate| aghist::llm::TodoCandidate {
                turn: candidate.citation.turn,
                role: candidate.role,
                kind: candidate.kind.slug(),
                snippet: candidate.snippet.as_str(),
            })
            .collect();
        let input = aghist::llm::TodoExtractionInput {
            provider: group.provider,
            session_id: &group.session_id,
            project: group.project.as_deref(),
            candidates: llm_candidates,
        };
        let extracted = aghist::llm::extract_for_session_todos(transport, config, &input)
            .map_err(|e| map_llm_error(&e))?;
        for todo in extracted {
            out.push(LlmTodoRow {
                citation: todo.citation,
                source: group.source.clone(),
                todo: todo.todo,
                source_snippet: todo.source_snippet,
                source_kind: todo.source_kind,
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
    started_at: chrono::DateTime<chrono::Utc>,
    candidates: Vec<TodoCandidate>,
}
