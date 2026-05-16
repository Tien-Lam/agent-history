use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::metadata::{self, Tag};
use aghist::output::OutputMode;

use super::super::super::cli::TagCommand;
use super::{metadata_error, open_metadata_db};

pub(crate) fn tag_dispatch(command: TagCommand, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    match command {
        TagCommand::Add { reference, tag } => {
            let row = metadata::tag_add(&conn, &reference, &tag).map_err(|e| metadata_error(&e))?;
            emit_tag_payload(&row, "added", mode)?;
            Ok(EXIT_OK)
        }
        TagCommand::List {
            reference,
            tag,
            json,
        } => {
            let mode = if json { OutputMode::Json } else { mode };
            let tags = metadata::tag_list(&conn, reference.as_deref(), tag.as_deref())
                .map_err(|e| metadata_error(&e))?;
            emit_tag_list(&tags, mode)?;
            if tags.is_empty() {
                Ok(EXIT_EMPTY)
            } else {
                Ok(EXIT_OK)
            }
        }
        TagCommand::Remove { reference, tag } => {
            let row =
                metadata::tag_remove(&conn, &reference, &tag).map_err(|e| metadata_error(&e))?;
            emit_tag_payload(&row, "removed", mode)?;
            Ok(EXIT_OK)
        }
    }
}

fn emit_tag_payload(tag: &Tag, action: &str, mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if mode.is_machine() {
        let payload = serde_json::json!({ action: tag });
        serde_json::to_writer(&mut out, &payload)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
        writeln!(out).ok();
    } else {
        writeln!(out, "{action} tag '{}' on {}", tag.tag, tag.session_ref).ok();
    }
    Ok(())
}

fn emit_tag_list(tags: &[Tag], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "tags": tags, "count": tags.len() });
            serde_json::to_writer(&mut out, &payload)
                .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
            writeln!(out).ok();
        }
        OutputMode::Ndjson => {
            for tag in tags {
                serde_json::to_writer(&mut out, tag).map_err(|e| {
                    ErrorEnvelope::new("io-error", format!("failed to emit NDJSON row: {e}"))
                })?;
                writeln!(out).ok();
            }
        }
        OutputMode::Human => {
            if tags.is_empty() {
                writeln!(out, "(no tags)").ok();
            } else {
                for tag in tags {
                    writeln!(
                        out,
                        "#{} {} [{}] (created {})",
                        tag.id, tag.session_ref, tag.tag, tag.created_at
                    )
                    .ok();
                }
            }
        }
    }
    Ok(())
}
