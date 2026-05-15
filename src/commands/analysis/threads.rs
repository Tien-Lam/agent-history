use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::Session;
use aghist::provider;

use crate::cli::FilterArgs;
use crate::commands::filtering::session_matches;
use crate::commands::text::truncate;

use super::common::map_llm_error;

#[derive(Clone, Copy)]
pub(crate) struct ThreadsCommandRequest<'a> {
    pub(crate) filters: &'a FilterArgs,
    pub(crate) gap_hours: i64,
    pub(crate) min_sessions: usize,
    pub(crate) limit: usize,
    pub(crate) force_json: bool,
    pub(crate) use_llm: bool,
    pub(crate) llm_model: Option<&'a str>,
    pub(crate) llm_max_sessions: usize,
}

pub(crate) fn threads_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    request: ThreadsCommandRequest<'_>,
) -> Result<i32, ErrorEnvelope> {
    let ThreadsCommandRequest {
        filters,
        gap_hours,
        min_sessions,
        limit,
        force_json,
        use_llm,
        llm_model,
        llm_max_sessions,
    } = request;
    if !use_llm && llm_model.is_some() {
        return Err(ErrorEnvelope::new("usage", "--llm-model requires --llm"));
    }
    if gap_hours < 0 {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("--gap-hours must be >= 0 (got {gap_hours})"),
        ));
    }

    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut sessions: Vec<Session> = Vec::new();
    for p in providers {
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        match p.discover_sessions() {
            Ok(found) => sessions.extend(
                found
                    .into_iter()
                    .filter(|s| session_matches(s, filters, project_needle.as_deref())),
            ),
            Err(e) => eprintln!("{}: error: {e}", p.provider()),
        }
    }

    if use_llm {
        return run_llm_threads(sessions, limit, llm_max_sessions, force_json, llm_model);
    }

    let opts = aghist::threads::ClusterOptions {
        gap: chrono::Duration::hours(gap_hours),
        min_sessions: min_sessions.max(1),
    };
    let mut threads = aghist::threads::cluster(&sessions, opts);

    if limit > 0 && threads.len() > limit {
        threads.truncate(limit);
    }

    if threads.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_threads_json(&mut out, &threads)
    } else {
        render_threads_human(&mut out, &threads)
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write threads output: {e}")))?;

    Ok(EXIT_OK)
}

/// LLM-driven topic clustering. Builds one session digest per local
/// `Session`, caps to the most recent `llm_max_sessions`, and routes
/// everything through a single Messages API call. Augments each returned
/// thread with derived metadata (providers / projects / `message_count`)
/// so the JSON shape stays compatible with the heuristic where possible.
fn run_llm_threads(
    sessions: Vec<Session>,
    limit: usize,
    llm_max_sessions: usize,
    force_json: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if sessions.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    // Cap to most-recent `llm_max_sessions` so input tokens stay bounded.
    // 0 means "no cap"; mirrors the rest of the CLI.
    let mut sorted = sessions;
    sorted.sort_by_key(|s| std::cmp::Reverse(s.started_at));
    if llm_max_sessions > 0 && sorted.len() > llm_max_sessions {
        sorted.truncate(llm_max_sessions);
    }

    let digests: Vec<aghist::llm::SessionDigest> = sorted
        .iter()
        .map(|s| aghist::llm::SessionDigest {
            provider: s.provider,
            session_id: s.id.clone(),
            project: s.project_name.clone(),
            started_at: s.started_at,
            ended_at: s.ended_at,
            summary: s.summary.clone(),
        })
        .collect();

    let mut config = aghist::llm::LlmConfig::from_env().map_err(|e| map_llm_error(&e))?;
    if let Some(model) = llm_model {
        config = config.with_model(model.to_string());
    }
    let transport = aghist::llm::UreqTransport::new(config.timeout);

    let raw = aghist::llm::extract_threads(&transport, &config, &digests)
        .map_err(|e| map_llm_error(&e))?;

    // Index sessions by `<provider-slug>/<session-id>` so we can stitch
    // derived metadata (providers, message_count, ...) onto each thread.
    let mut by_ref: std::collections::HashMap<String, &Session> =
        std::collections::HashMap::with_capacity(sorted.len());
    for s in &sorted {
        by_ref.insert(s.session_ref().to_string(), s);
    }

    let mut rows: Vec<LlmThreadRow> = Vec::with_capacity(raw.len());
    for t in raw {
        let mut providers_set: std::collections::BTreeSet<&'static str> =
            std::collections::BTreeSet::new();
        let mut branches_set: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        let mut projects_set: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        let mut message_count: usize = 0;
        for r in &t.member_refs {
            if let Some(s) = by_ref.get(r) {
                providers_set.insert(s.provider.slug());
                if let Some(b) = s.git_branch.as_deref().filter(|x| !x.is_empty()) {
                    branches_set.insert(b.to_string());
                }
                if let Some(p) = s.project_name.as_deref() {
                    projects_set.insert(p.to_string());
                }
                message_count = message_count.saturating_add(s.message_count);
            }
        }
        let id = llm_thread_id(&t.topic_summary, t.member_refs.first().map(String::as_str));
        rows.push(LlmThreadRow {
            id,
            topic_summary: t.topic_summary,
            member_refs: t.member_refs,
            time_span: t.time_span,
            providers: providers_set.into_iter().map(str::to_string).collect(),
            projects: projects_set.into_iter().collect(),
            branches: branches_set.into_iter().collect(),
            message_count,
        });
    }

    rows.sort_by(|a, b| {
        b.time_span
            .start
            .cmp(&a.time_span.start)
            .then_with(|| a.topic_summary.cmp(&b.topic_summary))
    });
    if limit > 0 && rows.len() > limit {
        rows.truncate(limit);
    }
    if rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_llm_threads_json(&mut out, &rows)
    } else {
        render_llm_threads_human(&mut out, &rows)
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write threads output: {e}")))?;

    Ok(EXIT_OK)
}

/// Stable short id for an LLM-grouped thread: FNV-1a over
/// `<topic_summary>|<first_member_ref>`. Mirrors the heuristic id format
/// (`th-<hex16>`) so consumers can format-discriminate.
fn llm_thread_id(topic: &str, first_ref: Option<&str>) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in topic.as_bytes() {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h ^= u64::from(b'|');
    h = h.wrapping_mul(0x100_0000_01b3);
    for byte in first_ref.unwrap_or("").as_bytes() {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("th-{h:016x}")
}

#[derive(serde::Serialize)]
struct LlmThreadRow {
    id: String,
    topic_summary: String,
    member_refs: Vec<String>,
    time_span: aghist::llm::TimeSpan,
    providers: Vec<String>,
    projects: Vec<String>,
    branches: Vec<String>,
    message_count: usize,
}

fn render_llm_threads_json<W: io::Write>(out: &mut W, rows: &[LlmThreadRow]) -> io::Result<()> {
    let payload = serde_json::json!({
        "threads": rows,
        "count": rows.len(),
        "mode": "llm",
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_llm_threads_human<W: io::Write>(out: &mut W, rows: &[LlmThreadRow]) -> io::Result<()> {
    writeln!(
        out,
        "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  TOPIC",
        "START (UTC)", "END (UTC)", "SESS", "MSGS", "ID"
    )?;
    for r in rows {
        let started = r.time_span.start.format("%Y-%m-%d %H:%M:%S").to_string();
        let ended = r.time_span.end.format("%Y-%m-%d %H:%M:%S").to_string();
        let topic = truncate(&r.topic_summary, 60);
        writeln!(
            out,
            "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  {topic}",
            started,
            ended,
            r.member_refs.len(),
            r.message_count,
            truncate(&r.id, 24),
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} thread(s)", rows.len())?;
    Ok(())
}

fn render_threads_json<W: io::Write>(
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

fn render_threads_human<W: io::Write>(
    out: &mut W,
    threads: &[aghist::threads::Thread],
) -> io::Result<()> {
    writeln!(
        out,
        "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  PROJECT",
        "STARTED (UTC)", "ENDED (UTC)", "SESS", "MSGS", "ID"
    )?;
    for t in threads {
        let started = t.started_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let ended = t.ended_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let project = t.project.as_deref().unwrap_or("(unknown)");
        let project = truncate(project, 40);
        writeln!(
            out,
            "{:<19}  {:<19}  {:>5}  {:>4}  {:<24}  {project}",
            started,
            ended,
            t.session_count,
            t.message_count,
            truncate(&t.id, 24),
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Total: {} thread(s)", threads.len())?;
    Ok(())
}
