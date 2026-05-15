use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::{CitationRef, Message, Provider, Role};
use aghist::provider;
use chrono::{DateTime, Utc};

use crate::cli::FilterArgs;
use crate::commands::filtering::{message_matches, session_matches};
use crate::commands::text::truncate;

use super::common::map_llm_error;

/// Run the heuristic across all matching sessions, returning unsorted rows.
fn collect_decision_rows(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    project_needle: Option<&str>,
    session_needle: Option<&str>,
    threshold: f32,
) -> Vec<DecisionRow> {
    let mut rows: Vec<DecisionRow> = Vec::new();
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
            if !session_matches(&session, filters, project_needle) {
                continue;
            }
            if let Some(needle) = session_needle {
                if !session.id.0.starts_with(needle) {
                    continue;
                }
            }
            let Ok(messages) = p.load_messages(&session) else {
                continue;
            };
            let scored: Vec<(usize, &Message)> = messages
                .iter()
                .enumerate()
                .filter(|(_, m)| message_matches(m, filters))
                .collect();
            for (idx, msg) in scored {
                let turn = u32::try_from(idx + 1).unwrap_or(u32::MAX);
                let cands = aghist::decisions::extract_from_message(msg, turn, threshold);
                for c in cands {
                    let Some(citation) = aghist::model::CitationRef::new(
                        session.provider,
                        session.id.clone(),
                        c.turn,
                    ) else {
                        continue;
                    };
                    rows.push(DecisionRow {
                        citation,
                        candidate: c,
                        project: session.project_name.clone(),
                        started_at: session.started_at,
                    });
                }
            }
        }
    }
    rows
}

#[derive(Clone, Copy)]
pub(crate) struct DecisionsCommandRequest<'a> {
    pub(crate) session_filter: Option<&'a str>,
    pub(crate) threshold: f32,
    pub(crate) limit: usize,
    pub(crate) force_json: bool,
    pub(crate) filters: &'a FilterArgs,
    pub(crate) use_llm: bool,
    pub(crate) llm_model: Option<&'a str>,
}

pub(crate) fn decisions_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    request: DecisionsCommandRequest<'_>,
) -> Result<i32, ErrorEnvelope> {
    let DecisionsCommandRequest {
        session_filter,
        threshold,
        limit,
        force_json,
        filters,
        use_llm,
        llm_model,
    } = request;
    if !use_llm && llm_model.is_some() {
        return Err(ErrorEnvelope::new("usage", "--llm-model requires --llm"));
    }
    if !threshold.is_finite() || threshold < 0.0 {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("--threshold must be a non-negative finite number (got {threshold})"),
        ));
    }

    // If --session was given as a full citation ref, drop the trailing
    // `#turn` so it can match the session id; we extract decisions across
    // the whole session regardless of the cited turn.
    let session_needle = session_filter.map(|s| {
        let trimmed = s.trim();
        let without_turn = trimmed.rsplit_once('#').map_or(trimmed, |(head, _)| head);
        // Strip leading provider segment if present (`<slug>/<id>` → `<id>`).
        without_turn
            .split_once('/')
            .map_or(without_turn, |(_, rest)| rest)
            .to_string()
    });

    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut rows = collect_decision_rows(
        providers,
        filters,
        project_needle.as_deref(),
        session_needle.as_deref(),
        threshold,
    );

    rows.sort_by(|a, b| {
        b.candidate
            .score
            .partial_cmp(&a.candidate.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.started_at.cmp(&a.started_at))
            .then_with(|| a.citation.session_id.0.cmp(&b.citation.session_id.0))
            .then_with(|| a.candidate.turn.cmp(&b.candidate.turn))
    });

    if use_llm {
        return run_llm_decisions(rows, limit, force_json, llm_model);
    }

    if rows.len() > limit {
        rows.truncate(limit);
    }

    if rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    if want_json {
        print_decisions_json(&rows).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?;
    } else {
        print_decisions_table(&rows);
    }
    Ok(EXIT_OK)
}

/// Route heuristic candidates through the LLM and emit structured decisions.
///
/// Groups rows by `(provider, session_id)` and issues one Messages API call
/// per session. The system prompt is cache-controlled, so calls 2..N pay
/// near-zero on the static prompt tokens. Falls through to `EXIT_EMPTY` if
/// no decisions survive.
fn run_llm_decisions(
    rows: Vec<DecisionRow>,
    limit: usize,
    force_json: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let mut config = aghist::llm::LlmConfig::from_env().map_err(|e| map_llm_error(&e))?;
    if let Some(model) = llm_model {
        config = config.with_model(model.to_string());
    }
    let transport = aghist::llm::UreqTransport::new(config.timeout);

    let groups = group_by_session(rows);
    let mut out = run_extraction(&transport, &config, groups)?;

    out.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| a.citation.session_id.0.cmp(&b.citation.session_id.0))
            .then_with(|| a.citation.turn.cmp(&b.citation.turn))
    });
    if out.len() > limit {
        out.truncate(limit);
    }
    if out.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    if force_json || !io::stdout().is_terminal() {
        print_llm_decisions_json(&out).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?;
    } else {
        print_llm_decisions_table(&out);
    }
    Ok(EXIT_OK)
}

/// Group heuristic rows by `(provider, session_id)`, preserving first-seen
/// order so the API call sequence stays predictable.
fn group_by_session(rows: Vec<DecisionRow>) -> Vec<SessionGroup> {
    let mut order: Vec<(Provider, aghist::model::SessionId)> = Vec::new();
    let mut grouped: std::collections::HashMap<(Provider, aghist::model::SessionId), SessionGroup> =
        std::collections::HashMap::new();
    for row in rows {
        let key = (row.citation.provider, row.citation.session_id.clone());
        let entry = grouped.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            SessionGroup {
                provider: row.citation.provider,
                session_id: row.citation.session_id.clone(),
                project: row.project.clone(),
                started_at: row.started_at,
                candidates: Vec::new(),
            }
        });
        entry.candidates.push(GroupedCandidate {
            turn: row.candidate.turn,
            role: row.candidate.role,
            snippet: row.candidate.snippet,
        });
    }
    let mut out = Vec::with_capacity(order.len());
    for key in order {
        if let Some(g) = grouped.remove(&key) {
            out.push(g);
        }
    }
    out
}

fn run_extraction<T: aghist::llm::LlmTransport + ?Sized>(
    transport: &T,
    config: &aghist::llm::LlmConfig,
    groups: Vec<SessionGroup>,
) -> Result<Vec<LlmRow>, ErrorEnvelope> {
    let mut out = Vec::new();
    for group in groups {
        let candidates: Vec<aghist::llm::Candidate> = group
            .candidates
            .iter()
            .map(|c| aghist::llm::Candidate {
                turn: c.turn,
                role: c.role,
                snippet: c.snippet.as_str(),
            })
            .collect();
        let input = aghist::llm::ExtractionInput {
            provider: group.provider,
            session_id: &group.session_id,
            project: group.project.as_deref(),
            candidates,
        };
        let extracted = aghist::llm::extract_for_session(transport, config, &input)
            .map_err(|e| map_llm_error(&e))?;
        for ed in extracted {
            out.push(LlmRow {
                citation: ed.citation,
                decision: ed.decision,
                source_snippet: ed.source_snippet,
                project: group.project.clone(),
                started_at: group.started_at,
            });
        }
    }
    Ok(out)
}

struct SessionGroup {
    provider: Provider,
    session_id: aghist::model::SessionId,
    project: Option<String>,
    started_at: DateTime<Utc>,
    candidates: Vec<GroupedCandidate>,
}

struct GroupedCandidate {
    turn: u32,
    role: Role,
    snippet: String,
}

struct LlmRow {
    citation: CitationRef,
    decision: aghist::llm::StructuredDecision,
    source_snippet: Option<String>,
    project: Option<String>,
    started_at: DateTime<Utc>,
}

fn print_llm_decisions_table(rows: &[LlmRow]) {
    println!("{:<36}  {:<60}  RATIONALE", "REF", "SUMMARY");
    for row in rows {
        let r = row.citation.to_string();
        let r = truncate(&r, 36);
        let summary = truncate(&row.decision.summary, 60);
        let rationale = truncate(&row.decision.rationale, 80);
        println!("{r:<36}  {summary:<60}  {rationale}");
    }
}

fn print_llm_decisions_json(rows: &[LlmRow]) -> std::io::Result<()> {
    use std::io::Write as _;

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
        .map(|r| JsonRow {
            reference: r.citation.to_string(),
            provider: r.citation.provider,
            session_id: r.citation.session_id.0.as_str(),
            turn: r.citation.turn,
            summary: r.decision.summary.as_str(),
            rationale: r.decision.rationale.as_str(),
            alternatives: &r.decision.alternatives,
            source_snippet: r.source_snippet.as_deref(),
            project: r.project.as_deref(),
            started_at: r.started_at,
        })
        .collect();

    let payload = Payload {
        count: decisions.len(),
        mode: "llm",
        decisions,
    };
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &payload)?;
    writeln!(stdout)
}

struct DecisionRow {
    citation: aghist::model::CitationRef,
    candidate: aghist::decisions::DecisionCandidate,
    project: Option<String>,
    started_at: DateTime<Utc>,
}

fn print_decisions_table(rows: &[DecisionRow]) {
    println!("{:<6}  {:<36}  {:<24}  SNIPPET", "SCORE", "REF", "MARKERS");
    for row in rows {
        let r = row.citation.to_string();
        let r = truncate(&r, 36);
        let markers = row.candidate.markers.join(",");
        let markers = truncate(&markers, 24);
        let snippet = truncate(&row.candidate.snippet, 80);
        println!(
            "{:<6.2}  {:<36}  {:<24}  {}",
            row.candidate.score, r, markers, snippet
        );
    }
}

fn print_decisions_json(rows: &[DecisionRow]) -> std::io::Result<()> {
    #[derive(serde::Serialize)]
    struct JsonRow<'a> {
        #[serde(rename = "ref")]
        reference: String,
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
        .map(|r| JsonRow {
            reference: r.citation.to_string(),
            provider: r.citation.provider,
            session_id: r.citation.session_id.0.as_str(),
            turn: r.citation.turn,
            role: r.candidate.role,
            score: r.candidate.score,
            markers: &r.candidate.markers,
            snippet: r.candidate.snippet.as_str(),
            project: r.project.as_deref(),
            timestamp: r.candidate.timestamp,
            started_at: r.started_at,
        })
        .collect();

    let payload = Payload {
        count: decisions.len(),
        decisions,
    };
    serde_json::to_writer(io::stdout().lock(), &payload).map_err(std::io::Error::other)?;
    println!();
    Ok(())
}
