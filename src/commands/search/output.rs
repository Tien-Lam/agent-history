use std::collections::HashMap;
use std::io::{self, Write};

use aghist::dto::{SearchEnvelope, SearchHitJson, SearchMeta};
use aghist::federated;
use aghist::model::Session;
use aghist::output::write_json_line;
use aghist::search;

use super::super::filtering::strip_turn_suffix;
use super::super::text::truncate;
use super::SearchHitRow;

pub(super) fn print_search_json(
    hits: &[SearchHitRow],
    sessions: &HashMap<String, &Session>,
    source_by_session: &HashMap<String, String>,
    hit_refs: &HashMap<String, String>,
    total: usize,
    next_cursor: Option<&str>,
    engine: &str,
) -> io::Result<()> {
    let rows: Vec<SearchHitJson> = hits
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

    let doc = SearchEnvelope {
        hits: rows,
        meta: SearchMeta::new(total, next_cursor, engine),
    };
    let mut out = io::stdout().lock();
    write_json_line(&mut out, &doc)
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
    write_json_line(out, &row)
}

fn json_hit<'a>(
    hit: &'a search::SearchHit,
    explanation: Option<&'a search::Explanation>,
    sessions: &'a HashMap<String, &'a Session>,
    source_by_session: &'a HashMap<String, String>,
    hit_refs: Option<&'a HashMap<String, String>>,
) -> SearchHitJson {
    match hit.kind {
        search::HitKind::Note => SearchHitJson::from_hit(
            hit,
            None,
            aghist::dto::source_from_note_ref(hit.note_session_ref.as_deref()),
            hit.note_session_ref.clone(),
            None,
            explanation,
        ),
        search::HitKind::Message => {
            let session = sessions.get(hit.session_key.as_str()).copied();
            let source = source_by_session
                .get(hit.session_key.as_str())
                .map_or(federated::LOCAL_SOURCE, String::as_str);
            SearchHitJson::from_hit(
                hit,
                session,
                source,
                hit_refs.and_then(|refs| refs.get(&hit.message_key).cloned()),
                None,
                explanation,
            )
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
            aghist::dto::source_from_note_ref(hit.note_session_ref.as_deref())
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
