use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::metadata::{self, Note};
use aghist::output::OutputMode;

use super::super::super::cli::NoteCommand;
use super::{metadata_error, open_metadata_db};

pub(crate) fn note_dispatch(command: NoteCommand, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    match command {
        NoteCommand::Add {
            reference,
            body,
            body_file,
            stdin,
        } => {
            let body = read_note_body(body.as_deref(), body_file.as_deref(), stdin)?;
            let note =
                metadata::note_add(&conn, &reference, &body).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "added", mode)?;
            Ok(EXIT_OK)
        }
        NoteCommand::List { reference, json } => {
            let mode = if json { OutputMode::Json } else { mode };
            let notes =
                metadata::note_list(&conn, reference.as_deref()).map_err(|e| metadata_error(&e))?;
            emit_note_list(&notes, mode)?;
            if notes.is_empty() {
                Ok(EXIT_EMPTY)
            } else {
                Ok(EXIT_OK)
            }
        }
        NoteCommand::Edit {
            id,
            body,
            body_file,
            stdin,
        } => {
            let body = read_note_body(body.as_deref(), body_file.as_deref(), stdin)?;
            let note = metadata::note_edit(&conn, id, &body).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "updated", mode)?;
            Ok(EXIT_OK)
        }
        NoteCommand::Remove { id } => {
            let note = metadata::note_remove(&conn, id).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "removed", mode)?;
            Ok(EXIT_OK)
        }
    }
}

fn read_note_body(
    body: Option<&str>,
    body_file: Option<&std::path::Path>,
    stdin: bool,
) -> Result<String, ErrorEnvelope> {
    use std::io::Read;
    if let Some(b) = body {
        return Ok(b.to_string());
    }
    let mut buf = String::new();
    if stdin {
        io::stdin().read_to_string(&mut buf).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to read note body from stdin: {e}"),
            )
        })?;
        return Ok(buf);
    }
    if let Some(path) = body_file {
        if path == std::path::Path::new("-") {
            io::stdin().read_to_string(&mut buf).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to read note body from stdin: {e}"),
                )
            })?;
        } else {
            buf = std::fs::read_to_string(path).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to read note body from {}: {e}", path.display()),
                )
            })?;
        }
        return Ok(buf);
    }
    Err(ErrorEnvelope::new(
        "usage",
        "note body required: pass --body, --body-file, or --stdin",
    ))
}

fn emit_note_payload(note: &Note, action: &str, mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if mode.is_machine() {
        let payload = serde_json::json!({ action: note });
        serde_json::to_writer(&mut out, &payload)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
        writeln!(out).ok();
    } else {
        writeln!(out, "{action} note {} on {}", note.id, note.session_ref).ok();
        for line in note.body.lines() {
            writeln!(out, "  {line}").ok();
        }
    }
    Ok(())
}

fn emit_note_list(notes: &[Note], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "notes": notes, "count": notes.len() });
            serde_json::to_writer(&mut out, &payload)
                .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
            writeln!(out).ok();
        }
        OutputMode::Ndjson => {
            for note in notes {
                serde_json::to_writer(&mut out, note).map_err(|e| {
                    ErrorEnvelope::new("io-error", format!("failed to emit NDJSON row: {e}"))
                })?;
                writeln!(out).ok();
            }
        }
        OutputMode::Human => {
            if notes.is_empty() {
                writeln!(out, "(no notes)").ok();
            } else {
                for note in notes {
                    writeln!(
                        out,
                        "#{} {} (created {}, updated {})",
                        note.id, note.session_ref, note.created_at, note.updated_at
                    )
                    .ok();
                    for line in note.body.lines() {
                        writeln!(out, "  {line}").ok();
                    }
                }
            }
        }
    }
    Ok(())
}
