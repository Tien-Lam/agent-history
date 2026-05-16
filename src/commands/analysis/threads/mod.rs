use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::provider;

use crate::cli::FilterArgs;

mod collect;
mod llm;
mod output;

use collect::collect_sessions;
use llm::run_llm_threads;
use output::{render_threads_human, render_threads_json};

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

    let sessions = collect_sessions(providers, filters);

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
