use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::model::{CitationRef, Message, Provider, Role, Session};
use aghist::output::write_json_line;
use aghist::services::lookup as lookup_service;
use aghist::{provider, query_scope};

use super::super::cli::ShowFormat;
use super::discovery::federated_discovery_for_commands;

pub(crate) fn show_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    raw_ref: &str,
    format: ShowFormat,
    include_context: u32,
) -> Result<i32, ErrorEnvelope> {
    let discovery = federated_discovery_for_commands(providers, scope);
    let target = lookup_service::load_citation_by_selector(
        providers,
        &discovery,
        raw_ref,
        include_context as usize,
    )?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match format {
        ShowFormat::Md => render_show_md(
            &mut out,
            &target.citation_ref,
            &target.session,
            &target.messages,
            target.start_idx,
            target.target_idx,
        ),
        ShowFormat::Json => render_show_json(
            &mut out,
            &target.citation_ref,
            &target.citation,
            &target.session,
            &target.messages,
            target.start_idx,
            target.target_idx,
        ),
        ShowFormat::Text => render_show_text(
            &mut out,
            &target.citation_ref,
            &target.messages,
            target.start_idx,
            target.target_idx,
        ),
    }
    .map_err(|e| ErrorEnvelope::io("failed to write show output", e))?;

    Ok(EXIT_OK)
}

fn render_show_md<W: io::Write>(
    out: &mut W,
    display_ref: &str,
    session: &Session,
    slice: &[Message],
    start_idx: usize,
    target_idx: usize,
) -> io::Result<()> {
    writeln!(out, "# {display_ref}")?;
    if let Some(project) = &session.project_name {
        writeln!(out, "_{project}_")?;
    }
    writeln!(out)?;
    for (i, msg) in slice.iter().enumerate() {
        let turn_no = start_idx + i + 1;
        let marker = if start_idx + i == target_idx {
            " ←"
        } else {
            ""
        };
        writeln!(out, "## Turn {turn_no} — {}{marker}\n", msg.role)?;
        write_blocks_text(out, msg)?;
        writeln!(out)?;
    }
    Ok(())
}

fn render_show_json<W: io::Write>(
    out: &mut W,
    display_ref: &str,
    citation: &CitationRef,
    session: &Session,
    slice: &[Message],
    start_idx: usize,
    target_idx: usize,
) -> io::Result<()> {
    #[derive(serde::Serialize)]
    struct ShowMessage<'a> {
        turn: usize,
        is_target: bool,
        role: Role,
        content: &'a [aghist::model::ContentBlock],
        timestamp: chrono::DateTime<chrono::Utc>,
    }
    #[derive(serde::Serialize)]
    struct ShowOut<'a> {
        #[serde(rename = "ref")]
        reference: String,
        provider: Provider,
        session_id: &'a str,
        project: Option<&'a str>,
        target_turn: u32,
        messages: Vec<ShowMessage<'a>>,
    }

    let messages: Vec<ShowMessage> = slice
        .iter()
        .enumerate()
        .map(|(i, m)| ShowMessage {
            turn: start_idx + i + 1,
            is_target: start_idx + i == target_idx,
            role: m.role,
            content: &m.content,
            timestamp: m.timestamp,
        })
        .collect();

    let payload = ShowOut {
        reference: display_ref.to_string(),
        provider: citation.provider,
        session_id: session.id.0.as_str(),
        project: session.project_name.as_deref(),
        target_turn: citation.turn,
        messages,
    };

    write_json_line(out, &payload)
}

fn render_show_text<W: io::Write>(
    out: &mut W,
    display_ref: &str,
    slice: &[Message],
    start_idx: usize,
    target_idx: usize,
) -> io::Result<()> {
    writeln!(out, "{display_ref}")?;
    for (i, msg) in slice.iter().enumerate() {
        let turn_no = start_idx + i + 1;
        let marker = if start_idx + i == target_idx {
            " (target)"
        } else {
            ""
        };
        writeln!(out, "--- Turn {turn_no} — {}{marker} ---", msg.role)?;
        write_blocks_text(out, msg)?;
    }
    Ok(())
}

fn write_blocks_text<W: io::Write>(out: &mut W, msg: &Message) -> io::Result<()> {
    use aghist::model::ContentBlock;
    for block in &msg.content {
        match block {
            ContentBlock::Text(t) => writeln!(out, "{t}")?,
            ContentBlock::CodeBlock { language, code } => {
                let lang = language.as_deref().unwrap_or("");
                writeln!(out, "```{lang}\n{code}\n```")?;
            }
            ContentBlock::ToolUse(tool) => {
                writeln!(out, "[tool: {}]\n{}", tool.name, tool.arguments)?;
            }
            ContentBlock::ToolResult(result) => {
                let status = if result.success { "ok" } else { "err" };
                writeln!(out, "[tool-result {status}]\n{}", result.output)?;
            }
            ContentBlock::Thinking(t) => writeln!(out, "[thinking] {t}")?,
            ContentBlock::Error(t) => writeln!(out, "[error] {t}")?,
        }
    }
    Ok(())
}
