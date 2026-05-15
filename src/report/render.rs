use std::fmt::Write as _;

use super::ReportEnvelope;

/// Render the report as a Markdown document suitable for pasting into a
/// journal or weekly review. The shape is stable; tests assert on
/// distinctive substrings.
#[must_use]
pub fn render_markdown(env: &ReportEnvelope) -> String {
    let mut out = String::new();
    write_report_heading(&mut out, env);
    write_top_projects(&mut out, env);
    write_decisions(&mut out, env);
    write_todos(&mut out, env);
    write_threads(&mut out, env);
    out
}

fn write_report_heading(out: &mut String, env: &ReportEnvelope) {
    let start = env.window.started_at.format("%Y-%m-%d");
    let end = env.window.ended_at.format("%Y-%m-%d");
    let days = env.window.days;
    let _ = writeln!(
        out,
        "# Weekly summary — {start} → {end} ({days} day{plural})",
        plural = if days == 1 { "" } else { "s" },
    );
    let _ = writeln!(out);

    let cost = env
        .token_usage
        .cost_usd
        .map_or_else(|| "—".to_string(), |c| format!("${c:.4}"));
    let _ = writeln!(
        out,
        "**Activity:** {} session(s) across {} project(s), {} message(s).",
        env.session_count, env.project_count, env.message_count,
    );
    let _ = writeln!(
        out,
        "**Tokens:** {} total · cost {cost}.",
        env.token_usage.total_tokens,
    );
    let _ = writeln!(out);
}

fn write_top_projects(out: &mut String, env: &ReportEnvelope) {
    let _ = writeln!(
        out,
        "## Top projects ({} of {})",
        env.top_projects.len(),
        env.meta.projects_total,
    );
    if env.top_projects.is_empty() {
        let _ = writeln!(out, "_No project activity in this window._");
    } else {
        for (i, p) in env.top_projects.iter().enumerate() {
            let p_cost = p
                .cost_usd
                .map_or_else(|| "—".to_string(), |c| format!("${c:.4}"));
            let _ = writeln!(
                out,
                "{n}. **{name}** — {sess} session(s), {msg} message(s), {tok} token(s), {cost}",
                n = i + 1,
                name = p.project,
                sess = p.session_count,
                msg = p.message_count,
                tok = p.total_tokens,
                cost = p_cost,
            );
        }
    }
    let _ = writeln!(out);
}

fn write_decisions(out: &mut String, env: &ReportEnvelope) {
    let _ = writeln!(
        out,
        "## Decisions ({} of {})",
        env.decisions.len(),
        env.meta.decisions_total,
    );
    if env.decisions.is_empty() {
        let _ = writeln!(
            out,
            "_No decision candidates above threshold {:.1}._",
            env.meta.decisions_threshold
        );
    } else {
        for d in &env.decisions {
            let _ = writeln!(
                out,
                "- [{score:.1}] {snippet} (`{ref_}`)",
                score = d.score,
                snippet = strip_newlines(&d.snippet),
                ref_ = d.reference,
            );
        }
    }
    let _ = writeln!(out);
}

fn write_todos(out: &mut String, env: &ReportEnvelope) {
    let _ = writeln!(
        out,
        "## Open TODOs ({} of {})",
        env.todos.len(),
        env.meta.todos_total,
    );
    if env.todos.is_empty() {
        let _ = writeln!(out, "_No open TODOs surfaced in this window._");
    } else {
        for t in &env.todos {
            let _ = writeln!(
                out,
                "- [{kind}] {snippet} (`{ref_}`)",
                kind = t.kind.slug(),
                snippet = strip_newlines(&t.snippet),
                ref_ = t.reference,
            );
        }
    }
    let _ = writeln!(out);
}

fn write_threads(out: &mut String, env: &ReportEnvelope) {
    let _ = writeln!(
        out,
        "## Threads ({} of {}; gap {}h)",
        env.threads.len(),
        env.meta.threads_total,
        env.meta.thread_gap_hours,
    );
    if env.threads.is_empty() {
        let _ = writeln!(out, "_No threads in this window._");
    } else {
        for t in &env.threads {
            let project = t.project.clone().unwrap_or_else(|| "(unknown)".to_string());
            let _ = writeln!(
                out,
                "- **{project}** — {start} → {end}, {sess} session(s), {msg} message(s)",
                start = t.started_at.format("%Y-%m-%d %H:%M"),
                end = t.ended_at.format("%Y-%m-%d %H:%M"),
                sess = t.session_count,
                msg = t.message_count,
            );
        }
    }
}

fn strip_newlines(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::strip_newlines;

    #[test]
    fn strip_newlines_collapses_whitespace() {
        assert_eq!(strip_newlines("a\nb\rc"), "a b c");
        assert_eq!(strip_newlines("  a   b  "), "a b");
    }
}
