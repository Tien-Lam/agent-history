use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::{CitationRef, ContentBlock, Message, Provider, Role, Session};
use aghist::provider;
use aghist::todos::{self, TodoCandidate, TodoKind};
use chrono::{DateTime, Utc};

use super::super::cli::FilterArgs;
use super::filtering::{message_matches, session_matches};
use super::text::truncate;

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

// ── track command ─────────────────────────────────────────────────────────────

/// Scan all providers for sessions mentioning `topic`, returning up to `limit` with excerpts.
fn scan_topic_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    topic: &str,
    limit: usize,
) -> Vec<aghist::llm::TrackSession> {
    let needle = topic.to_lowercase();
    let mut matched: Vec<aghist::llm::TrackSession> = Vec::new();

    'outer: for p in providers {
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        let Ok(sessions) = p.discover_sessions() else {
            continue;
        };
        for session in sessions {
            if limit > 0 && matched.len() >= limit {
                break 'outer;
            }
            if filters.since.is_some_and(|s| session.started_at < s)
                || filters.until.is_some_and(|u| session.started_at > u)
            {
                continue;
            }
            if let Some(ref proj) = filters.project {
                let name = session.project_name.as_deref().unwrap_or("");
                if !name.to_lowercase().contains(&proj.to_lowercase()) {
                    continue;
                }
            }
            let Ok(messages) = p.load_messages(&session) else {
                continue;
            };
            let mut excerpts: Vec<String> = Vec::new();
            for msg in &messages {
                if excerpts.len() >= 3 {
                    break;
                }
                for block in &msg.content {
                    if let ContentBlock::Text(t) = block {
                        if t.to_lowercase().contains(&needle) {
                            let snippet = t.trim();
                            let short = if snippet.chars().count() > 200 {
                                format!("{}…", snippet.chars().take(199).collect::<String>())
                            } else {
                                snippet.to_string()
                            };
                            excerpts.push(short);
                            break;
                        }
                    }
                }
            }
            if excerpts.is_empty() {
                continue;
            }
            matched.push(aghist::llm::TrackSession {
                provider: p.provider(),
                session_id: session.id.clone(),
                started_at: session.started_at,
                excerpts,
            });
        }
    }
    matched
}

pub(crate) fn track_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
    topic: &str,
    limit: usize,
    force_json: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    use std::io::Write as _;
    let mut matched = scan_topic_sessions(providers, filters, topic, limit);

    if matched.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    matched.sort_by_key(|s| s.started_at);

    let mut config = aghist::llm::LlmConfig::from_env().map_err(|e| map_llm_error(&e))?;
    if let Some(model) = llm_model {
        config = config.with_model(model.to_string());
    }
    let transport = aghist::llm::UreqTransport::new(config.timeout);

    let events = aghist::llm::extract_track(&transport, &config, topic, &matched)
        .map_err(|e| map_llm_error(&e))?;

    if events.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    if want_json {
        let payload = serde_json::json!({
            "topic": topic,
            "sessions_scanned": matched.len(),
            "timeline": events,
        });
        let stdout = io::stdout();
        let mut out = stdout.lock();
        serde_json::to_writer(&mut out, &payload)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("json: {e}")))?;
        writeln!(out).map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
    } else {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        writeln!(out, "Topic: {topic}")
            .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
        writeln!(out, "Sessions scanned: {}", matched.len())
            .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
        writeln!(out).map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
        writeln!(
            out,
            "{:<10}  {:<42}  {:<12}  EVENT",
            "DATE", "REF", "DIRECTION"
        )
        .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
        for ev in &events {
            let ref_short = truncate(&ev.session_ref, 42);
            let event_short = truncate(&ev.event, 80);
            writeln!(
                out,
                "{:<10}  {:<42}  {:<12}  {}",
                ev.date, ref_short, ev.direction, event_short
            )
            .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
        }
        writeln!(out).map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
        writeln!(out, "Total: {} event(s)", events.len())
            .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
    }

    Ok(EXIT_OK)
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

fn map_llm_error(e: &aghist::llm::LlmError) -> ErrorEnvelope {
    use aghist::llm::LlmError;
    let env = ErrorEnvelope::new("llm-error", e.to_string());
    match e {
        LlmError::MissingApiKey => {
            env.with_hint("Set ANTHROPIC_API_KEY (or AGHIST_LLM_API_KEY) and re-run.")
        }
        LlmError::ApiStatus {
            status: 401 | 403, ..
        } => env.with_hint("Verify ANTHROPIC_API_KEY is valid and has access to the chosen model."),
        LlmError::ApiStatus { status: 429, .. } => {
            env.with_hint("Rate limited — retry with --limit lowered or wait and retry.")
        }
        _ => env,
    }
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
