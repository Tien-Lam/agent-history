use std::collections::HashSet;
use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::CitationRef;
use aghist::todos::{TodoCandidate, TodoKind};
use aghist::{provider, query_scope};
use chrono::{DateTime, Utc};

use crate::cli::FilterArgs;

mod collect;
mod llm;
mod output;

use collect::collect_federated_todo_candidates;
use llm::run_llm_todos;
use output::{render_todos_human, render_todos_json};

#[derive(Clone, Copy)]
pub(crate) struct TodosCommandRequest<'a> {
    pub(crate) filters: &'a FilterArgs,
    pub(crate) metadata_keys: Option<&'a HashSet<String>>,
    pub(crate) kinds: &'a [TodoKind],
    pub(crate) limit: usize,
    pub(crate) force_json: bool,
    pub(crate) use_llm: bool,
    pub(crate) llm_model: Option<&'a str>,
}

pub(crate) fn todos_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    request: TodosCommandRequest<'_>,
) -> Result<i32, ErrorEnvelope> {
    let TodosCommandRequest {
        filters,
        metadata_keys,
        kinds,
        limit,
        force_json,
        use_llm,
        llm_model,
    } = request;
    if !use_llm && llm_model.is_some() {
        return Err(ErrorEnvelope::new("usage", "--llm-model requires --llm"));
    }

    if use_llm {
        let all =
            collect_federated_todo_candidates(providers, scope, filters, metadata_keys, kinds);
        return run_llm_todos(all, limit, force_json, llm_model);
    }

    let mut all =
        collect_federated_todo_candidates(providers, scope, filters, metadata_keys, kinds);

    // Newest matches first — most useful for "what's still hanging?".
    all.sort_by(|a, b| {
        b.candidate
            .timestamp
            .cmp(&a.candidate.timestamp)
            .then_with(|| {
                a.candidate
                    .citation
                    .session_id
                    .0
                    .cmp(&b.candidate.citation.session_id.0)
            })
            .then_with(|| a.candidate.citation.turn.cmp(&b.candidate.citation.turn))
            .then_with(|| (a.candidate.kind as u8).cmp(&(b.candidate.kind as u8)))
    });

    if limit > 0 && all.len() > limit {
        all.truncate(limit);
    }

    if all.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_todos_json(&mut out, &all)
    } else {
        render_todos_human(&mut out, &all)
    }
    .map_err(|e| ErrorEnvelope::io("failed to write todos output", e))?;

    Ok(EXIT_OK)
}

/// LLM-mode todo row with full metadata for rendering.
struct LlmTodoRow {
    citation: CitationRef,
    source: String,
    todo: aghist::llm::StructuredTodo,
    source_snippet: Option<String>,
    source_kind: Option<String>,
    project: Option<String>,
    started_at: DateTime<Utc>,
}

impl LlmTodoRow {
    fn reference(&self) -> String {
        if self.source == aghist::federated::LOCAL_SOURCE {
            self.citation.to_string()
        } else {
            format!("{}:{}", self.source, self.citation)
        }
    }
}

struct TodoRow {
    candidate: TodoCandidate,
    source: String,
    project: Option<String>,
    started_at: DateTime<Utc>,
}

impl TodoRow {
    fn reference(&self) -> String {
        if self.source == aghist::federated::LOCAL_SOURCE {
            self.candidate.citation.to_string()
        } else {
            format!("{}:{}", self.source, self.candidate.citation)
        }
    }
}
