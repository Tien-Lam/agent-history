use std::io;

use crate::commands::text::truncate;

use super::{LlmTodoRow, TodoRow};

pub(super) fn render_todos_json<W: io::Write>(out: &mut W, todos: &[TodoRow]) -> io::Result<()> {
    let payload = serde_json::json!({
        "todos": todos.iter().map(|row| serde_json::json!({
            "ref": row.reference(),
            "source": row.source,
            "provider": row.candidate.citation.provider,
            "session_id": row.candidate.citation.session_id.0,
            "turn": row.candidate.citation.turn,
            "kind": row.candidate.kind,
            "snippet": row.candidate.snippet,
            "role": row.candidate.role,
            "timestamp": row.candidate.timestamp,
            "bd_id": row.candidate.bd_id,
        })).collect::<Vec<_>>(),
        "count": todos.len(),
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

pub(super) fn render_todos_human<W: io::Write>(out: &mut W, todos: &[TodoRow]) -> io::Result<()> {
    writeln!(
        out,
        "{:<14}  {:<19}  {:<46}  SNIPPET",
        "KIND", "WHEN (UTC)", "REF"
    )?;
    for row in todos {
        let candidate = &row.candidate;
        let when = candidate.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
        let reference = row.reference();
        let reference = truncate(&reference, 46);
        let snippet = truncate(&candidate.snippet, 80);
        writeln!(
            out,
            "{:<14}  {:<19}  {:<46}  {snippet}",
            candidate.kind.slug(),
            when,
            reference
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} candidate(s)", todos.len())?;
    Ok(())
}

pub(super) fn render_llm_todos_json<W: io::Write>(
    out: &mut W,
    rows: &[LlmTodoRow],
) -> io::Result<()> {
    let payload = serde_json::json!({
        "todos": rows.iter().map(|row| serde_json::json!({
            "ref": row.reference(),
            "source": row.source,
            "provider": row.citation.provider,
            "session_id": row.citation.session_id.0,
            "turn": row.citation.turn,
            "description": row.todo.description,
            "target_session": row.todo.target_session,
            "status_inferred": row.todo.status_inferred,
            "source_snippet": row.source_snippet,
            "source_kind": row.source_kind,
            "project": row.project,
            "started_at": row.started_at,
        })).collect::<Vec<_>>(),
        "count": rows.len(),
        "mode": "llm",
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

pub(super) fn render_llm_todos_human<W: io::Write>(
    out: &mut W,
    rows: &[LlmTodoRow],
) -> io::Result<()> {
    writeln!(
        out,
        "{:<8}  {:<46}  {:<48}  TARGET",
        "STATUS", "REF", "DESCRIPTION"
    )?;
    for row in rows {
        let status = match row.todo.status_inferred {
            aghist::llm::TodoStatus::Open => "open",
            aghist::llm::TodoStatus::Done => "done",
            aghist::llm::TodoStatus::Unclear => "unclear",
        };
        let reference = row.reference();
        let reference = truncate(&reference, 46);
        let description = truncate(&row.todo.description, 48);
        let target = row.todo.target_session.as_deref().unwrap_or("");
        writeln!(
            out,
            "{status:<8}  {reference:<46}  {description:<48}  {target}"
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} todo(s)", rows.len())?;
    Ok(())
}
