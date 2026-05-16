use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::{CitationRef, Provider};
use aghist::provider;
use aghist::todos::TodoKind;
use chrono::{DateTime, Utc};

use crate::cli::FilterArgs;

mod collect;
mod llm;
mod output;

use collect::collect_todo_candidates;
use llm::run_llm_todos;
use output::{render_todos_human, render_todos_json};

pub(crate) fn todos_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    kinds: &[TodoKind],
    limit: usize,
    force_json: bool,
    use_llm: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if !use_llm && llm_model.is_some() {
        return Err(ErrorEnvelope::new("usage", "--llm-model requires --llm"));
    }

    let (mut all, session_meta) = collect_todo_candidates(providers, filters, kinds, use_llm);

    if use_llm {
        return run_llm_todos(all, &session_meta, limit, force_json, llm_model);
    }

    // Newest matches first — most useful for "what's still hanging?".
    all.sort_by(|a, b| {
        b.timestamp
            .cmp(&a.timestamp)
            .then_with(|| a.citation.session_id.0.cmp(&b.citation.session_id.0))
            .then_with(|| a.citation.turn.cmp(&b.citation.turn))
            .then_with(|| (a.kind as u8).cmp(&(b.kind as u8)))
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
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write todos output: {e}")))?;

    Ok(EXIT_OK)
}

/// LLM-mode todo row with full metadata for rendering.
struct LlmTodoRow {
    citation: CitationRef,
    todo: aghist::llm::StructuredTodo,
    source_snippet: Option<String>,
    source_kind: Option<String>,
    project: Option<String>,
    started_at: DateTime<Utc>,
}

type SessionMetaMap = std::collections::HashMap<
    (Provider, aghist::model::SessionId),
    (Option<String>, DateTime<Utc>),
>;
