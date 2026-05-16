use std::io;

use crate::commands::text::truncate;

use super::{DecisionRow, LlmRow};

pub(super) fn render_llm_decisions_human<W: io::Write>(
    out: &mut W,
    rows: &[LlmRow],
) -> io::Result<()> {
    writeln!(out, "{:<36}  {:<60}  RATIONALE", "REF", "SUMMARY")?;
    for row in rows {
        let reference = row.citation.to_string();
        let reference = truncate(&reference, 36);
        let summary = truncate(&row.decision.summary, 60);
        let rationale = truncate(&row.decision.rationale, 80);
        writeln!(out, "{reference:<36}  {summary:<60}  {rationale}")?;
    }
    Ok(())
}

pub(super) fn render_llm_decisions_json<W: io::Write>(
    out: &mut W,
    rows: &[LlmRow],
) -> io::Result<()> {
    #[derive(serde::Serialize)]
    struct JsonRow<'a> {
        #[serde(rename = "ref")]
        reference: String,
        provider: aghist::model::Provider,
        session_id: &'a str,
        turn: u32,
        summary: &'a str,
        rationale: &'a str,
        alternatives: &'a [String],
        source_snippet: Option<&'a str>,
        project: Option<&'a str>,
        started_at: chrono::DateTime<chrono::Utc>,
    }

    #[derive(serde::Serialize)]
    struct Payload<'a> {
        decisions: Vec<JsonRow<'a>>,
        count: usize,
        mode: &'static str,
    }

    let decisions: Vec<JsonRow> = rows
        .iter()
        .map(|row| JsonRow {
            reference: row.citation.to_string(),
            provider: row.citation.provider,
            session_id: row.citation.session_id.0.as_str(),
            turn: row.citation.turn,
            summary: row.decision.summary.as_str(),
            rationale: row.decision.rationale.as_str(),
            alternatives: &row.decision.alternatives,
            source_snippet: row.source_snippet.as_deref(),
            project: row.project.as_deref(),
            started_at: row.started_at,
        })
        .collect();

    let payload = Payload {
        count: decisions.len(),
        mode: "llm",
        decisions,
    };
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)
}

pub(super) fn render_decisions_human<W: io::Write>(
    out: &mut W,
    rows: &[DecisionRow],
) -> io::Result<()> {
    writeln!(
        out,
        "{:<6}  {:<36}  {:<24}  SNIPPET",
        "SCORE", "REF", "MARKERS"
    )?;
    for row in rows {
        let reference = row.reference();
        let reference = truncate(&reference, 36);
        let markers = row.candidate.markers.join(",");
        let markers = truncate(&markers, 24);
        let snippet = truncate(&row.candidate.snippet, 80);
        writeln!(
            out,
            "{:<6.2}  {:<36}  {:<24}  {}",
            row.candidate.score, reference, markers, snippet
        )?;
    }
    Ok(())
}

pub(super) fn render_decisions_json<W: io::Write>(
    out: &mut W,
    rows: &[DecisionRow],
) -> io::Result<()> {
    #[derive(serde::Serialize)]
    struct JsonRow<'a> {
        #[serde(rename = "ref")]
        reference: String,
        source: &'a str,
        provider: aghist::model::Provider,
        session_id: &'a str,
        turn: u32,
        role: aghist::model::Role,
        score: f32,
        markers: &'a [String],
        snippet: &'a str,
        project: Option<&'a str>,
        timestamp: chrono::DateTime<chrono::Utc>,
        started_at: chrono::DateTime<chrono::Utc>,
    }

    #[derive(serde::Serialize)]
    struct Payload<'a> {
        decisions: Vec<JsonRow<'a>>,
        count: usize,
    }

    let decisions: Vec<JsonRow> = rows
        .iter()
        .map(|row| JsonRow {
            reference: row.reference(),
            source: row.source.as_str(),
            provider: row.citation.provider,
            session_id: row.citation.session_id.0.as_str(),
            turn: row.citation.turn,
            role: row.candidate.role,
            score: row.candidate.score,
            markers: &row.candidate.markers,
            snippet: row.candidate.snippet.as_str(),
            project: row.project.as_deref(),
            timestamp: row.candidate.timestamp,
            started_at: row.started_at,
        })
        .collect();

    let payload = Payload {
        count: decisions.len(),
        decisions,
    };
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)
}
