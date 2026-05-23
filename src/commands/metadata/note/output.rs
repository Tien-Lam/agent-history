use std::io;

use aghist::cli_error::ErrorEnvelope;
use aghist::metadata::Note;
use aghist::output::{write_json_line, OutputMode};

pub(super) fn emit_note_payload(
    note: &Note,
    action: &str,
    mode: OutputMode,
) -> Result<(), ErrorEnvelope> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_note_payload(&mut out, note, action, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write note output", e))
}

fn write_note_payload<W: io::Write>(
    out: &mut W,
    note: &Note,
    action: &str,
    mode: OutputMode,
) -> io::Result<()> {
    if mode.is_machine() {
        let payload = serde_json::json!({ action: note });
        write_json_line(out, &payload)?;
    } else {
        writeln!(out, "{action} note {} on {}", note.id, note.session_ref)?;
        for line in note.body.lines() {
            writeln!(out, "  {line}")?;
        }
    }
    Ok(())
}

pub(super) fn emit_note_list(notes: &[Note], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_note_list(&mut out, notes, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write note output", e))
}

fn write_note_list<W: io::Write>(out: &mut W, notes: &[Note], mode: OutputMode) -> io::Result<()> {
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "notes": notes, "count": notes.len() });
            write_json_line(out, &payload)?;
        }
        OutputMode::Ndjson => {
            for note in notes {
                write_json_line(out, note)?;
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
