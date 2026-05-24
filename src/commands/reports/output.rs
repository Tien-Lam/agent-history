use std::io;

use aghist::cli_error::ErrorEnvelope;
use aghist::output::write_json_line;

use super::super::text::truncate;

mod project;

pub(super) use project::render_project_human;

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
    write_json_line(out, &payload)
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
            "(showing {} of {} row(s) — raise --limit to include more)",
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
    write_json_line(out, report)
}

pub(super) fn render_report<W: io::Write>(
    out: &mut W,
    envelope: &aghist::report::ReportEnvelope,
    force_json: bool,
) -> Result<(), ErrorEnvelope> {
    if force_json {
        write_json_line(out, envelope)
            .map_err(|e| ErrorEnvelope::io("failed to write report", e))?;
    } else {
        let md = aghist::report::render_markdown(envelope);
        write!(out, "{md}").map_err(|e| ErrorEnvelope::io("failed to write report", e))?;
    }
    Ok(())
}
