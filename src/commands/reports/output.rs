use std::io;

use aghist::cli_error::ErrorEnvelope;

use super::super::text::truncate;

pub(super) fn render_usage_json<W: io::Write>(
    out: &mut W,
    report: &aghist::usage::UsageReport,
    group_by: aghist::usage::GroupBy,
    total_rows: usize,
) -> io::Result<()> {
    let payload = serde_json::json!({
        "rows": report.rows,
        "totals": report.totals,
        "meta": {
            "group_by": group_by.as_str(),
            "row_count": report.rows.len(),
            "total_row_count": total_rows,
        },
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

pub(super) fn render_usage_human<W: io::Write>(
    out: &mut W,
    report: &aghist::usage::UsageReport,
    group_by: aghist::usage::GroupBy,
    total_rows: usize,
) -> io::Result<()> {
    let key_header = match group_by {
        aghist::usage::GroupBy::Model => "MODEL",
        aghist::usage::GroupBy::Provider => "PROVIDER",
        aghist::usage::GroupBy::Project => "PROJECT",
    };
    writeln!(
        out,
        "{:<32}  {:>5}  {:>12}  {:>12}  {:>14}  COST",
        key_header, "SESS", "INPUT", "OUTPUT", "TOTAL TOKENS"
    )?;
    for row in &report.rows {
        let cost = row
            .cost_usd
            .map_or_else(|| "—".to_string(), |c| format!("${c:.4}"));
        writeln!(
            out,
            "{:<32}  {:>5}  {:>12}  {:>12}  {:>14}  {cost}",
            truncate(&row.key, 32),
            row.session_count,
            row.input_tokens,
            row.output_tokens,
            row.total_tokens,
        )?;
    }
    writeln!(out)?;
    let total_cost = report
        .totals
        .cost_usd
        .map_or_else(|| "—".to_string(), |c| format!("${c:.4}"));
    writeln!(
        out,
        "Total: {} session(s), {} message(s), {} token(s), cost {}",
        report.totals.session_count,
        report.totals.message_count,
        report.totals.total_tokens,
        total_cost,
    )?;
    if total_rows > report.rows.len() {
        writeln!(
            out,
            "(showing {} of {} row(s) — pass --limit 0 for all)",
            report.rows.len(),
            total_rows,
        )?;
    }
    Ok(())
}

pub(super) fn render_project_json<W: io::Write>(
    out: &mut W,
    report: &aghist::project::ProjectReport,
) -> io::Result<()> {
    serde_json::to_writer(&mut *out, report).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

pub(super) fn render_project_human<W: io::Write>(
    out: &mut W,
    report: &aghist::project::ProjectReport,
) -> io::Result<()> {
    render_project_header(out, report)?;
    render_project_decisions(out, report)?;
    render_project_todos(out, report)?;
    render_project_threads(out, report)?;
    render_project_files(out, report)?;
    render_project_time_of_day(out, report)
}

pub(super) fn render_report<W: io::Write>(
    out: &mut W,
    envelope: &aghist::report::ReportEnvelope,
    force_json: bool,
) -> Result<(), ErrorEnvelope> {
    if force_json {
        serde_json::to_writer(&mut *out, envelope)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to encode report: {e}")))?;
        writeln!(out)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write report: {e}")))?;
    } else {
        let md = aghist::report::render_markdown(envelope);
        write!(out, "{md}")
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write report: {e}")))?;
    }
    Ok(())
}

fn render_project_header<W: io::Write>(
    out: &mut W,
    report: &aghist::project::ProjectReport,
) -> io::Result<()> {
    writeln!(out, "Project: {}", report.query)?;
    if !report.matched_projects.is_empty() {
        writeln!(out, "Matched: {}", report.matched_projects.join(", "))?;
    }
    let cost = report
        .token_usage
        .cost_usd
        .map_or_else(|| "—".to_string(), |c| format!("${c:.4}"));
    writeln!(
        out,
        "{} session(s), {} message(s), {} token(s), cost {cost}",
        report.session_count, report.message_count, report.token_usage.total_tokens,
    )?;
    if let (Some(start), Some(end)) = (report.started_at, report.ended_at) {
        writeln!(
            out,
            "Active: {} → {}",
            start.format("%Y-%m-%d %H:%M:%SZ"),
            end.format("%Y-%m-%d %H:%M:%SZ"),
        )?;
    }
    writeln!(out)?;

    writeln!(
        out,
        "Tokens: in {} | out {} | cache_r {} | cache_w {}",
        report.token_usage.input_tokens,
        report.token_usage.output_tokens,
        report.token_usage.cache_read_tokens,
        report.token_usage.cache_write_tokens,
    )?;
    writeln!(out)?;
    Ok(())
}

fn render_project_decisions<W: io::Write>(
    out: &mut W,
    report: &aghist::project::ProjectReport,
) -> io::Result<()> {
    writeln!(
        out,
        "Decisions ({} of {}):",
        report.decisions.len(),
        report.meta.decisions_total,
    )?;
    if report.decisions.is_empty() {
        writeln!(out, "  (none)")?;
    }
    for d in &report.decisions {
        let snippet = truncate(&d.snippet, 80);
        writeln!(
            out,
            "  [{:>4.1}] {}  {snippet}",
            d.score,
            truncate(&d.reference, 36),
        )?;
    }
    writeln!(out)?;
    Ok(())
}

fn render_project_todos<W: io::Write>(
    out: &mut W,
    report: &aghist::project::ProjectReport,
) -> io::Result<()> {
    writeln!(
        out,
        "Todos ({} of {}):",
        report.todos.len(),
        report.meta.todos_total,
    )?;
    if report.todos.is_empty() {
        writeln!(out, "  (none)")?;
    }
    for t in &report.todos {
        let snippet = truncate(&t.snippet, 80);
        writeln!(
            out,
            "  [{:<12}] {}  {snippet}",
            t.kind.slug(),
            truncate(&t.reference, 36),
        )?;
    }
    writeln!(out)?;
    Ok(())
}

fn render_project_threads<W: io::Write>(
    out: &mut W,
    report: &aghist::project::ProjectReport,
) -> io::Result<()> {
    writeln!(
        out,
        "Threads ({} of {}; gap {}h):",
        report.threads.len(),
        report.meta.threads_total,
        report.meta.thread_gap_hours,
    )?;
    if report.threads.is_empty() {
        writeln!(out, "  (none)")?;
    }
    for t in &report.threads {
        writeln!(
            out,
            "  {} → {}  {} session(s), {} msg(s)",
            t.started_at.format("%Y-%m-%d %H:%M"),
            t.ended_at.format("%Y-%m-%d %H:%M"),
            t.session_count,
            t.message_count,
        )?;
    }
    writeln!(out)?;
    Ok(())
}

fn render_project_files<W: io::Write>(
    out: &mut W,
    report: &aghist::project::ProjectReport,
) -> io::Result<()> {
    writeln!(
        out,
        "Top files ({} of {}):",
        report.top_files.len(),
        report.meta.files_total,
    )?;
    if report.top_files.is_empty() {
        writeln!(out, "  (none)")?;
    }
    for f in &report.top_files {
        writeln!(out, "  {:>6}  {}", f.count, truncate(&f.path, 70))?;
    }
    writeln!(out)?;
    Ok(())
}

fn render_project_time_of_day<W: io::Write>(
    out: &mut W,
    report: &aghist::project::ProjectReport,
) -> io::Result<()> {
    writeln!(out, "Time of day (UTC, message counts):")?;
    let max = *report.time_of_day.iter().max().unwrap_or(&0);
    for (h, count) in report.time_of_day.iter().enumerate() {
        let bar_len = bar_cells(*count, max, 20);
        let bar = "█".repeat(bar_len);
        writeln!(out, "  {h:02}:00  {count:>6}  {bar}")?;
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
#[allow(clippy::cast_possible_wrap)]
fn bar_cells(count: u64, max: u64, max_cells: u64) -> usize {
    if max == 0 || count == 0 {
        return 0;
    }
    let ratio = (count as f64 / max as f64).min(1.0);
    let cells = (ratio * max_cells as f64).round() as u64;
    cells.min(max_cells) as usize
}
