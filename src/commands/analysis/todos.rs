use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::{CitationRef, Provider};
use aghist::provider;
use aghist::todos::{self, TodoCandidate, TodoKind};
use chrono::{DateTime, Utc};

use crate::cli::FilterArgs;
use crate::commands::filtering::{message_matches, session_matches};
use crate::commands::text::truncate;

use super::common::map_llm_error;

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

    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut all: Vec<TodoCandidate> = Vec::new();
    // For --llm: per-session metadata (project + started_at) keyed by
    // (provider, session_id). Built alongside `all` so we don't re-iterate
    // providers/sessions a second time in the LLM branch.
    let mut session_meta: SessionMetaMap = std::collections::HashMap::new();

    for p in providers {
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        let sessions = match p.discover_sessions() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: error: {e}", p.provider());
                continue;
            }
        };
        for session in sessions {
            if !session_matches(&session, filters, project_needle.as_deref()) {
                continue;
            }
            let Ok(messages) = p.load_messages(&session) else {
                continue;
            };
            let candidates =
                todos::extract_from_messages(p.provider(), &session.id, &messages, kinds);
            let mut session_emitted = false;
            for c in candidates {
                if filters.role.is_some() || filters.has_tool_call {
                    let turn_idx = (c.citation.turn as usize).saturating_sub(1);
                    let Some(msg) = messages.get(turn_idx) else {
                        continue;
                    };
                    if !message_matches(msg, filters) {
                        continue;
                    }
                }
                if let Some(since) = filters.since {
                    if c.timestamp < since {
                        continue;
                    }
                }
                if let Some(until) = filters.until {
                    if c.timestamp > until {
                        continue;
                    }
                }
                if use_llm && !session_emitted {
                    session_meta.insert(
                        (p.provider(), session.id.clone()),
                        (session.project_name.clone(), session.started_at),
                    );
                    session_emitted = true;
                }
                all.push(c);
            }
        }
    }

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

fn render_todos_json<W: io::Write>(out: &mut W, todos: &[TodoCandidate]) -> io::Result<()> {
    let payload = serde_json::json!({
        "todos": todos.iter().map(|c| serde_json::json!({
            "ref": c.citation.to_string(),
            "provider": c.citation.provider,
            "session_id": c.citation.session_id.0,
            "turn": c.citation.turn,
            "kind": c.kind,
            "snippet": c.snippet,
            "role": c.role,
            "timestamp": c.timestamp,
            "bd_id": c.bd_id,
        })).collect::<Vec<_>>(),
        "count": todos.len(),
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_todos_human<W: io::Write>(out: &mut W, todos: &[TodoCandidate]) -> io::Result<()> {
    writeln!(
        out,
        "{:<14}  {:<19}  {:<46}  SNIPPET",
        "KIND", "WHEN (UTC)", "REF"
    )?;
    for c in todos {
        let when = c.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
        let reference = c.citation.to_string();
        let reference = truncate(&reference, 46);
        let snippet = truncate(&c.snippet, 80);
        writeln!(
            out,
            "{:<14}  {:<19}  {:<46}  {snippet}",
            c.kind.slug(),
            when,
            reference
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} candidate(s)", todos.len())?;
    Ok(())
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

/// Route heuristic candidates through the LLM and emit structured todos.
///
type SessionMetaMap = std::collections::HashMap<
    (Provider, aghist::model::SessionId),
    (Option<String>, DateTime<Utc>),
>;

/// Groups candidates by `(provider, session_id)` and issues one Messages
/// API call per session. The system prompt is cache-controlled so calls
/// 2..N pay near-zero on the static prompt tokens. Falls through to
/// `EXIT_EMPTY` when no todos survive.
fn run_llm_todos(
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

    // Group candidates by (provider, session_id), preserving first-seen
    // order so the API call sequence stays predictable across runs.
    let mut order: Vec<(Provider, aghist::model::SessionId)> = Vec::new();
    let mut grouped: std::collections::HashMap<
        (Provider, aghist::model::SessionId),
        Vec<TodoCandidate>,
    > = std::collections::HashMap::new();
    for c in candidates {
        let key = (c.citation.provider, c.citation.session_id.clone());
        if !grouped.contains_key(&key) {
            order.push(key.clone());
        }
        grouped.entry(key).or_default().push(c);
    }

    let mut out: Vec<LlmTodoRow> = Vec::new();
    for key in order {
        let group = grouped.remove(&key).unwrap_or_default();
        let (project, started_at) = session_meta
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (None, DateTime::<Utc>::from_timestamp(0, 0).unwrap()));
        let llm_candidates: Vec<aghist::llm::TodoCandidate> = group
            .iter()
            .map(|c| aghist::llm::TodoCandidate {
                turn: c.citation.turn,
                role: c.role,
                kind: c.kind.slug(),
                snippet: c.snippet.as_str(),
            })
            .collect();
        let input = aghist::llm::TodoExtractionInput {
            provider: key.0,
            session_id: &key.1,
            project: project.as_deref(),
            candidates: llm_candidates,
        };
        let extracted = aghist::llm::extract_for_session_todos(&transport, &config, &input)
            .map_err(|e| map_llm_error(&e))?;
        for et in extracted {
            out.push(LlmTodoRow {
                citation: et.citation,
                todo: et.todo,
                source_snippet: et.source_snippet,
                source_kind: et.source_kind,
                project: project.clone(),
                started_at,
            });
        }
    }

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

fn render_llm_todos_json<W: io::Write>(out: &mut W, rows: &[LlmTodoRow]) -> io::Result<()> {
    let payload = serde_json::json!({
        "todos": rows.iter().map(|r| serde_json::json!({
            "ref": r.citation.to_string(),
            "provider": r.citation.provider,
            "session_id": r.citation.session_id.0,
            "turn": r.citation.turn,
            "description": r.todo.description,
            "target_session": r.todo.target_session,
            "status_inferred": r.todo.status_inferred,
            "source_snippet": r.source_snippet,
            "source_kind": r.source_kind,
            "project": r.project,
            "started_at": r.started_at,
        })).collect::<Vec<_>>(),
        "count": rows.len(),
        "mode": "llm",
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_llm_todos_human<W: io::Write>(out: &mut W, rows: &[LlmTodoRow]) -> io::Result<()> {
    writeln!(
        out,
        "{:<8}  {:<46}  {:<48}  TARGET",
        "STATUS", "REF", "DESCRIPTION"
    )?;
    for r in rows {
        let status = match r.todo.status_inferred {
            aghist::llm::TodoStatus::Open => "open",
            aghist::llm::TodoStatus::Done => "done",
            aghist::llm::TodoStatus::Unclear => "unclear",
        };
        let reference = r.citation.to_string();
        let reference = truncate(&reference, 46);
        let description = truncate(&r.todo.description, 48);
        let target = r.todo.target_session.as_deref().unwrap_or("");
        writeln!(
            out,
            "{status:<8}  {reference:<46}  {description:<48}  {target}"
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} todo(s)", rows.len())?;
    Ok(())
}
