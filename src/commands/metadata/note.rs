use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::metadata::{self, Note};
use aghist::output::OutputMode;

use super::super::super::cli::NoteCommand;
use super::{json_to_io_error, metadata_error, open_metadata_db};

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
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_note_payload(&mut out, note, action, mode)
        .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write note output: {e}")))
}

fn write_note_payload<W: io::Write>(
    out: &mut W,
    note: &Note,
    action: &str,
    mode: OutputMode,
) -> io::Result<()> {
    if mode.is_machine() {
        let payload = serde_json::json!({ action: note });
        serde_json::to_writer(&mut *out, &payload).map_err(json_to_io_error)?;
        writeln!(out)?;
    } else {
        writeln!(out, "{action} note {} on {}", note.id, note.session_ref)?;
        for line in note.body.lines() {
            writeln!(out, "  {line}")?;
        }
    }
    Ok(())
}

fn emit_note_list(notes: &[Note], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_note_list(&mut out, notes, mode)
        .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write note output: {e}")))
}

fn write_note_list<W: io::Write>(out: &mut W, notes: &[Note], mode: OutputMode) -> io::Result<()> {
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "notes": notes, "count": notes.len() });
            serde_json::to_writer(&mut *out, &payload).map_err(json_to_io_error)?;
            writeln!(out)?;
        }
        OutputMode::Ndjson => {
            for note in notes {
                serde_json::to_writer(&mut *out, note).map_err(json_to_io_error)?;
                writeln!(out)?;
            }
        }
        OutputMode::Human => {
            if notes.is_empty() {
                writeln!(out, "(no notes)")?;
            } else {
                for note in notes {
                    writeln!(
                        out,
                        "#{} {} (created {}, updated {})",
                        note.id, note.session_ref, note.created_at, note.updated_at
                    )?;
                    for line in note.body.lines() {
                        writeln!(out, "  {line}")?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingWriter;

    impl io::Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn note() -> Note {
        Note {
            id: 7,
            session_ref: "claude-code/session#2".to_string(),
            body: "line one\nline two".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn note_payload_surfaces_writer_errors() {
        let mut out = FailingWriter;
        let err = write_note_payload(&mut out, &note(), "added", OutputMode::Human).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn note_list_surfaces_json_newline_errors() {
        struct NewlineFails(Vec<u8>);

        impl io::Write for NewlineFails {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                if buf == b"\n" {
                    Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
                } else {
                    self.0.extend_from_slice(buf);
                    Ok(buf.len())
                }
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut out = NewlineFails(Vec::new());
        let err = write_note_list(&mut out, &[note()], OutputMode::Json).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }
}
