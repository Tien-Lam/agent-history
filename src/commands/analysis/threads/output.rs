use std::io;

use crate::commands::text::truncate;

use super::LlmThreadRow;

pub(super) fn render_llm_threads_json<W: io::Write>(
    out: &mut W,
    rows: &[LlmThreadRow],
) -> io::Result<()> {
    let payload = serde_json::json!({
        "threads": rows,
        "count": rows.len(),
        "mode": "llm",
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

pub(super) fn render_llm_threads_human<W: io::Write>(
    out: &mut W,
    rows: &[LlmThreadRow],
) -> io::Result<()> {
    writeln!(
        out,
        "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  TOPIC",
        "START (UTC)", "END (UTC)", "SESS", "MSGS", "ID"
    )?;
    for row in rows {
        let started = row.time_span.start.format("%Y-%m-%d %H:%M:%S").to_string();
        let ended = row.time_span.end.format("%Y-%m-%d %H:%M:%S").to_string();
        let topic = truncate(&row.topic_summary, 60);
        writeln!(
            out,
            "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  {topic}",
            started,
            ended,
            row.member_refs.len(),
            row.message_count,
            truncate(&row.id, 24),
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} thread(s)", rows.len())?;
    Ok(())
}

pub(super) fn render_threads_json<W: io::Write>(
    out: &mut W,
    threads: &[aghist::threads::Thread],
) -> io::Result<()> {
    let payload = serde_json::json!({
        "threads": threads,
        "count": threads.len(),
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

pub(super) fn render_threads_human<W: io::Write>(
    out: &mut W,
    threads: &[aghist::threads::Thread],
) -> io::Result<()> {
    writeln!(
        out,
        "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  PROJECT",
        "STARTED (UTC)", "ENDED (UTC)", "SESS", "MSGS", "ID"
    )?;
    for thread in threads {
        let started = thread.started_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let ended = thread.ended_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let project = thread.project.as_deref().unwrap_or("(unknown)");
        let project = truncate(project, 40);
        writeln!(
            out,
            "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  {project}",
            started,
            ended,
            thread.session_count,
            thread.message_count,
            truncate(&thread.id, 24),
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} thread(s)", threads.len())?;
    Ok(())
}
