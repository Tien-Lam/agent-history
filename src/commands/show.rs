use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::model::{CitationRef, Message, Provider, QualifiedCitationRef, Role, Session};
use aghist::{config, federated, provider};

use super::super::cli::ShowFormat;
use super::discovery::federated_discovery_for_commands;

pub(crate) fn show_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    raw_ref: &str,
    format: ShowFormat,
    include_context: u32,
) -> Result<i32, ErrorEnvelope> {
    let parsed = parse_show_ref(raw_ref)?;
    let citation = parsed.citation;

    let discovery = federated_discovery_for_commands(providers);
    let wanted_source = parsed.source.as_deref().unwrap_or(federated::LOCAL_SOURCE);
    let display_ref = QualifiedCitationRef::new(
        (wanted_source != federated::LOCAL_SOURCE).then(|| wanted_source.to_string()),
        citation.clone(),
    )
    .to_string();
    let session = discovery
        .sessions
        .iter()
        .find(|s| {
            s.provider == citation.provider
                && s.id == citation.session_id
                && discovery.source_of_session(s) == wanted_source
        })
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "session-not-found",
                format!(
                    "session '{}' not found in provider '{}' from source '{}'",
                    citation.session_id,
                    citation.provider.slug(),
                    wanted_source
                ),
            )
            .with_hint("Run `aghist search --json` to get source-aware refs.")
        })?;

    let messages = provider::load_messages_for_session(session, providers).map_err(|e| {
        ErrorEnvelope::new(
            "provider-error",
            format!("failed to load messages for {}: {e}", session.id.0),
        )
    })?;

    let total = messages.len();
    let turn = citation.turn as usize;
    if turn > total {
        return Err(ErrorEnvelope::new(
            "session-not-found",
            format!("turn {turn} out of range: session has {total} message(s)"),
        )
        .with_hint("Use `aghist export` to inspect the full session, or pick a smaller turn."));
    }

    let target_idx = turn - 1;
    let ctx = include_context as usize;
    let start_idx = target_idx.saturating_sub(ctx);
    let end_idx = (target_idx + ctx + 1).min(total);
    let slice = &messages[start_idx..end_idx];

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match format {
        ShowFormat::Md => render_show_md(
            &mut out,
            &display_ref,
            session,
            slice,
            start_idx,
            target_idx,
        ),
        ShowFormat::Json => render_show_json(
            &mut out,
            &display_ref,
            &citation,
            session,
            slice,
            start_idx,
            target_idx,
        ),
        ShowFormat::Text => render_show_text(&mut out, &display_ref, slice, start_idx, target_idx),
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write show output: {e}")))?;

    Ok(EXIT_OK)
}

fn parse_show_ref(raw_ref: &str) -> Result<QualifiedCitationRef, ErrorEnvelope> {
    let slash = raw_ref.find('/');
    let colon = raw_ref.find(':');
    let has_source = matches!((colon, slash), (Some(c), Some(s)) if c < s);
    if has_source {
        let (source, rest) = raw_ref.split_once(':').expect("colon detected above");
        config::validate_source_name(source)
            .map_err(|message| ErrorEnvelope::new("usage", message))?;
        let citation = rest
            .parse()
            .map_err(|e: aghist::model::CitationParseError| {
                ErrorEnvelope::new("usage", format!("invalid ref '{raw_ref}': {e}")).with_hint(
                    "Format: <provider-slug>/<session-id>#<turn> or <source>:<provider-slug>/<session-id>#<turn>.",
                )
            })?;
        Ok(QualifiedCitationRef::new(
            Some(source.to_string()),
            citation,
        ))
    } else {
        raw_ref.parse::<QualifiedCitationRef>().map_err(|e| {
            ErrorEnvelope::new("usage", format!("invalid ref '{raw_ref}': {e}")).with_hint(
                "Format: <provider-slug>/<session-id>#<turn> or <source>:<provider-slug>/<session-id>#<turn>.",
            )
        })
    }
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

    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
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
