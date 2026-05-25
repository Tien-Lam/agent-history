use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::Session;

use crate::commands::discovery::{qualified_session_ref, source_for_session};

use super::super::common::{llm_config_from_env, map_llm_error, should_emit_json};
use super::output::{render_llm_threads_human, render_llm_threads_json};
use super::LlmThreadRow;

/// LLM-driven topic clustering. Builds one source-qualified digest per
/// `Session`, caps to the most recent `llm_max_sessions`, and routes
/// everything through a single Messages API call. Augments each returned
/// thread with derived metadata (providers / projects / `message_count`)
/// so the JSON shape stays compatible with the heuristic where possible.
pub(super) fn run_llm_threads(
    sessions: Vec<Session>,
    source_by_session: &std::collections::HashMap<String, String>,
    limit: usize,
    llm_max_sessions: usize,
    force_json: bool,
    llm_model: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    if sessions.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let sorted = most_recent_sessions(sessions, llm_max_sessions);
    let digests = session_digests(&sorted, source_by_session);

    let config = llm_config_from_env(llm_model)?;
    let transport = aghist::llm::UreqTransport::new(config.timeout);

    let raw = aghist::llm::extract_threads(&transport, &config, &digests)
        .map_err(|e| map_llm_error(&e))?;
    let mut rows = stitch_thread_rows(raw, &sorted, source_by_session);

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

    let want_json = should_emit_json(force_json);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if want_json {
        render_llm_threads_json(&mut out, &rows)
    } else {
        render_llm_threads_human(&mut out, &rows)
    }
    .map_err(|e| ErrorEnvelope::io("failed to write threads output", e))?;

    Ok(EXIT_OK)
}

fn most_recent_sessions(mut sessions: Vec<Session>, llm_max_sessions: usize) -> Vec<Session> {
    sessions.sort_by_key(|session| std::cmp::Reverse(session.started_at));
    let max_sessions = llm_max_sessions.max(1);
    if sessions.len() > max_sessions {
        sessions.truncate(max_sessions);
    }
    sessions
}

fn session_digests(
    sessions: &[Session],
    source_by_session: &std::collections::HashMap<String, String>,
) -> Vec<aghist::llm::SessionDigest> {
    sessions
        .iter()
        .map(|session| {
            let source = source_for_session(source_by_session, session);
            aghist::llm::SessionDigest {
                source: (source != aghist::federated::LOCAL_SOURCE).then(|| source.to_string()),
                provider: session.provider,
                session_id: session.id.clone(),
                project: session.project_name.clone(),
                started_at: session.started_at,
                ended_at: session.ended_at,
                summary: session.summary.clone(),
            }
        })
        .collect()
}

fn stitch_thread_rows(
    raw: Vec<aghist::llm::StructuredThread>,
    sessions: &[Session],
    source_by_session: &std::collections::HashMap<String, String>,
) -> Vec<LlmThreadRow> {
    let mut by_ref: std::collections::HashMap<String, &Session> =
        std::collections::HashMap::with_capacity(sessions.len());
    for session in sessions {
        by_ref.insert(qualified_session_ref(source_by_session, session), session);
    }

    let mut rows = Vec::with_capacity(raw.len());
    for thread in raw {
        let mut providers_set: std::collections::BTreeSet<&'static str> =
            std::collections::BTreeSet::new();
        let mut branches_set: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        let mut projects_set: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        let mut message_count: usize = 0;
        for reference in &thread.member_refs {
            if let Some(session) = by_ref.get(reference) {
                providers_set.insert(session.provider.slug());
                if let Some(branch) = session.git_branch.as_deref().filter(|x| !x.is_empty()) {
                    branches_set.insert(branch.to_string());
                }
                if let Some(project) = session.project_name.as_deref() {
                    projects_set.insert(project.to_string());
                }
                message_count = message_count.saturating_add(session.message_count);
            }
        }
        let id = llm_thread_id(
            &thread.topic_summary,
            thread.member_refs.first().map(String::as_str),
        );
        rows.push(LlmThreadRow {
            id,
            topic_summary: thread.topic_summary,
            member_refs: thread.member_refs,
            time_span: thread.time_span,
            providers: providers_set.into_iter().map(str::to_string).collect(),
            projects: projects_set.into_iter().collect(),
            branches: branches_set.into_iter().collect(),
            message_count,
        });
    }
    rows
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
