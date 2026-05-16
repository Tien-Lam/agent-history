use std::collections::HashMap;
use std::io::{self, Write};

use aghist::model::Session;
use aghist::search;
use aghist::{federated, model};

use super::super::filtering::strip_turn_suffix;
use super::super::text::truncate;
use super::SearchHitRow;

#[derive(serde::Serialize)]
struct JsonHit<'a> {
    kind: &'static str,
    session_id: &'a str,
    message_id: &'a str,
    score: f32,
    snippet: &'a str,
    provider: Option<model::Provider>,
    project: Option<&'a str>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    source: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    note_id: Option<i64>,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    ref_: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    explanation: Option<&'a search::Explanation>,
}

pub(super) fn print_search_json(
    hits: &[SearchHitRow],
    sessions: &HashMap<String, &Session>,
    source_by_session: &HashMap<String, String>,
    hit_refs: &HashMap<String, String>,
    total: usize,
    next_cursor: Option<&str>,
    engine: &str,
) -> io::Result<()> {
    let rows: Vec<JsonHit> = hits
        .iter()
        .map(|(h, explain)| {
            json_hit(
                h,
                explain.as_ref(),
                sessions,
                source_by_session,
                Some(hit_refs),
            )
        })
        .collect();

    let doc = serde_json::json!({
        "hits": rows,
        "meta": { "next_cursor": next_cursor, "total": total, "engine": engine },
    });
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, &doc)?;
    writeln!(out)?;
    Ok(())
}

pub(super) fn print_search_table(
    hits: &[SearchHitRow],
    sessions: &HashMap<String, &Session>,
    source_by_session: &HashMap<String, String>,
    next_cursor: Option<&str>,
) -> io::Result<()> {
    let mut out = io::stdout().lock();
    let any_remote = hits.iter().any(|(h, _)| {
        source_by_session
            .get(h.session_key.as_str())
            .is_some_and(|s| s != federated::LOCAL_SOURCE)
    });

    if any_remote {
        writeln!(
            out,
            "{:<6}  {:<16}  {:<12}  {:<20}  {:<10}  {:<14}  SNIPPET",
            "SCORE", "STARTED", "PROVIDER", "PROJECT", "SOURCE", "SESSION"
        )?;
    } else {
        writeln!(
            out,
            "{:<6}  {:<16}  {:<12}  {:<20}  {:<14}  SNIPPET",
            "SCORE", "STARTED", "PROVIDER", "PROJECT", "SESSION"
        )?;
    }
    for (h, explain) in hits {
        write_table_row(
            &mut out,
            h,
            explain.as_ref(),
            sessions,
            source_by_session,
            any_remote,
        )?;
    }
    if let Some(token) = next_cursor {
        writeln!(
            out,
            "\n(more results — pass --cursor {token} for the next page)"
        )?;
    }
    Ok(())
}

pub(super) fn write_watch_hit<W: Write>(
    out: &mut W,
    hit: &search::SearchHit,
    sessions: &HashMap<String, &Session>,
    source_by_session: &HashMap<String, String>,
) -> io::Result<()> {
    let row = json_hit(hit, None, sessions, source_by_session, None);
    serde_json::to_writer(&mut *out, &row)?;
    out.write_all(b"\n")?;
    Ok(())
}

fn json_hit<'a>(
    hit: &'a search::SearchHit,
    explanation: Option<&'a search::Explanation>,
    sessions: &'a HashMap<String, &'a Session>,
    source_by_session: &'a HashMap<String, String>,
    hit_refs: Option<&'a HashMap<String, String>>,
) -> JsonHit<'a> {
    match hit.kind {
        search::HitKind::Note => JsonHit {
            kind: search::HitKind::Note.slug(),
            session_id: &hit.session_id,
            message_id: &hit.message_id,
            score: hit.score,
            snippet: &hit.snippet,
            provider: None,
            project: None,
            started_at: None,
            source: note_source(hit.note_session_ref.as_deref()),
            note_id: hit.note_id,
            ref_: hit.note_session_ref.as_deref(),
            explanation,
        },
        search::HitKind::Message => {
            let session = sessions.get(hit.session_key.as_str()).copied();
            let source = source_by_session
                .get(hit.session_key.as_str())
                .map_or(federated::LOCAL_SOURCE, String::as_str);
            JsonHit {
                kind: search::HitKind::Message.slug(),
                session_id: &hit.session_id,
                message_id: &hit.message_id,
                score: hit.score,
                snippet: &hit.snippet,
                provider: session.map(|s| s.provider),
                project: session.and_then(|s| s.project_name.as_deref()),
                started_at: session.map(|s| s.started_at),
                source,
                note_id: None,
                ref_: hit_refs.and_then(|refs| refs.get(&hit.message_key).map(String::as_str)),
                explanation,
            }
        }
    }
}

fn write_table_row<W: Write>(
    out: &mut W,
    hit: &search::SearchHit,
    explanation: Option<&search::Explanation>,
    sessions: &HashMap<String, &Session>,
    source_by_session: &HashMap<String, String>,
    any_remote: bool,
) -> io::Result<()> {
    let is_note = matches!(hit.kind, search::HitKind::Note);
    let session = sessions.get(hit.session_key.as_str()).copied();
    let started = if is_note {
        String::new()
    } else {
        session
            .map(|s| s.started_at.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default()
    };
    let provider = if is_note {
        "note"
    } else {
        session.map_or("", |s| s.provider.as_str())
    };
    let project_owned = if is_note {
        hit.note_session_ref
            .as_deref()
            .map(|r| strip_turn_suffix(r).to_string())
            .unwrap_or_default()
    } else {
        session
            .and_then(|s| s.project_name.as_deref())
            .unwrap_or("")
            .to_string()
    };
    let project = truncate(&project_owned, 20);
    let session_label = if is_note {
        hit.note_id
            .map_or_else(String::new, |id| format!("note#{id}"))
    } else {
        hit.session_id.clone()
    };
    let session_short = truncate(&session_label, 14);
    let snippet = truncate(&hit.snippet, 80);
    if any_remote {
        let source = if is_note {
            note_source(hit.note_session_ref.as_deref())
        } else {
            source_by_session
                .get(hit.session_key.as_str())
                .map_or(federated::LOCAL_SOURCE, String::as_str)
        };
        let source = truncate(source, 10);
        writeln!(
            out,
            "{:<6.2}  {:<16}  {:<12}  {:<20}  {:<10}  {:<14}  {}",
            hit.score, started, provider, project, source, session_short, snippet
        )?;
    } else {
        writeln!(
            out,
            "{:<6.2}  {:<16}  {:<12}  {:<20}  {:<14}  {}",
            hit.score, started, provider, project, session_short, snippet
        )?;
    }
    if let Some(explanation) = explanation {
        for line in explanation.to_pretty_json().lines() {
            writeln!(out, "    {line}")?;
        }
    }
    Ok(())
}

fn note_source(reference: Option<&str>) -> &str {
    let Some(reference) = reference else {
        return federated::LOCAL_SOURCE;
    };
    let slash = reference.find('/');
    let colon = reference.find(':');
    match (colon, slash) {
        (Some(c), Some(s)) if c < s => &reference[..c],
        _ => federated::LOCAL_SOURCE,
    }
}
