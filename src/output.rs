use std::fmt::Write;

use chrono::Utc;
use clap::ValueEnum;
use comfy_table::{ContentArrangement, Table};

use crate::model::{Message, MessageRole, Session};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Plain,
}

/// Format a list of sessions for display.
#[must_use]
pub fn format_sessions(sessions: &[Session], format: OutputFormat) -> String {
    match format {
        OutputFormat::Table => format_sessions_table(sessions),
        OutputFormat::Json => serde_json::to_string_pretty(sessions).unwrap_or_default(),
        OutputFormat::Plain => format_sessions_plain(sessions),
    }
}

fn format_sessions_table(sessions: &[Session]) -> String {
    if sessions.is_empty() {
        return "No sessions found.".to_string();
    }

    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        "ID", "Provider", "Project", "Started", "Msgs", "Summary",
    ]);

    for session in sessions {
        let age = format_relative_time(session.started_at);
        let project = session
            .project_path
            .rsplit('/')
            .next()
            .unwrap_or(&session.project_path);
        let summary = truncate(&session.summary, 60);

        table.add_row(vec![
            session.id.short().to_string(),
            session.provider.clone(),
            project.to_string(),
            age,
            session.message_count.to_string(),
            summary,
        ]);
    }

    table.to_string()
}

fn format_sessions_plain(sessions: &[Session]) -> String {
    sessions
        .iter()
        .map(|s| {
            format!(
                "{} {} {} ({} messages)",
                s.id.short(),
                s.provider,
                s.project_path,
                s.message_count,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format messages for the `show` command.
#[must_use]
pub fn format_messages(
    messages: &[Message],
    format: OutputFormat,
    show_tools: bool,
    show_thinking: bool,
) -> String {
    match format {
        OutputFormat::Json => serde_json::to_string_pretty(messages).unwrap_or_default(),
        OutputFormat::Table | OutputFormat::Plain => {
            format_messages_plain(messages, show_tools, show_thinking)
        }
    }
}

fn format_messages_plain(messages: &[Message], show_tools: bool, show_thinking: bool) -> String {
    let mut output = String::new();

    for msg in messages {
        // Skip tool-call-only messages if --no-tools
        if !show_tools && !msg.tool_calls.is_empty() {
            continue;
        }

        // Role header
        let role_label = match msg.role {
            MessageRole::User => "── User ",
            MessageRole::Assistant => "── Assistant ",
            MessageRole::System => "── System ",
        };

        output.push_str(role_label);
        if let Some(ref model) = msg.model {
            output.push('(');
            output.push_str(model);
            output.push(')');
        }
        output.push_str(" ──\n");

        // Thinking block
        if show_thinking && let Some(ref thinking) = msg.thinking {
            output.push_str("<thinking>\n");
            output.push_str(thinking);
            output.push_str("\n</thinking>\n\n");
        }

        // Main content
        if !msg.content.is_empty() {
            output.push_str(&msg.content);
            output.push('\n');
        }

        // Tool calls
        if show_tools {
            for tc in &msg.tool_calls {
                let _ = writeln!(output, "  [tool: {}] {}", tc.name, tc.input_preview);
            }
        }

        output.push('\n');
    }

    output
}

/// Search result for display.
pub struct SearchResult<'a> {
    pub session: &'a Session,
    pub matches: Vec<SearchMatch>,
}

pub struct SearchMatch {
    pub message_index: usize,
    pub role: MessageRole,
    pub line: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// Format search results for display.
#[must_use]
pub fn format_search_results(results: &[SearchResult<'_>]) -> String {
    if results.is_empty() {
        return "No matches found.".to_string();
    }

    let mut output = String::new();

    for result in results {
        let project = result
            .session
            .project_path
            .rsplit('/')
            .next()
            .unwrap_or(&result.session.project_path);
        let _ = writeln!(
            output,
            "━━ {} ({} / {}) ━━",
            result.session.id.short(),
            result.session.provider,
            project,
        );

        for m in &result.matches {
            for ctx in &m.context_before {
                let _ = writeln!(output, "  {ctx}");
            }
            let _ = writeln!(output, "  >> [{}] {}", m.role, m.line);
            for ctx in &m.context_after {
                let _ = writeln!(output, "  {ctx}");
            }
            output.push('\n');
        }
    }

    output
}

fn format_relative_time(dt: chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(dt);

    if duration.num_days() > 30 {
        dt.format("%Y-%m-%d").to_string()
    } else if duration.num_days() > 0 {
        format!("{}d ago", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{}m ago", duration.num_minutes())
    } else {
        "just now".to_string()
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}
