use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::metadata::{self, Star};
use aghist::output::{write_json_line, OutputMode};

use super::{metadata_error, open_metadata_db};

pub(crate) fn star_command(reference: &str, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    let row = metadata::star_add(&conn, reference).map_err(|e| metadata_error(&e))?;
    emit_star_payload(&row, "starred", mode)?;
    Ok(EXIT_OK)
}

pub(crate) fn unstar_command(reference: &str, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    let row = metadata::star_remove(&conn, reference).map_err(|e| metadata_error(&e))?;
    emit_star_payload(&row, "unstarred", mode)?;
    Ok(EXIT_OK)
}

pub(crate) fn stars_list(reference: Option<&str>, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    let stars = metadata::star_list(&conn, reference).map_err(|e| metadata_error(&e))?;
    emit_star_list(&stars, mode)?;
    if stars.is_empty() {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

fn emit_star_payload(star: &Star, action: &str, mode: OutputMode) -> Result<(), ErrorEnvelope> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_star_payload(&mut out, star, action, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write star output", e))
}

fn write_star_payload<W: io::Write>(
    out: &mut W,
    star: &Star,
    action: &str,
    mode: OutputMode,
) -> io::Result<()> {
    if mode.is_machine() {
        let payload = serde_json::json!({ action: star });
        write_json_line(out, &payload)?;
    } else {
        writeln!(out, "{action} {}", star.session_ref)?;
    }
    Ok(())
}

fn emit_star_list(stars: &[Star], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_star_list(&mut out, stars, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write star output", e))
}

fn write_star_list<W: io::Write>(out: &mut W, stars: &[Star], mode: OutputMode) -> io::Result<()> {
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "stars": stars, "count": stars.len() });
            write_json_line(out, &payload)?;
        }
        OutputMode::Ndjson => {
            for star in stars {
                write_json_line(out, star)?;
            }
        }
        OutputMode::Human => {
            if stars.is_empty() {
                writeln!(out, "(no stars)")?;
            } else {
                for star in stars {
                    writeln!(out, "* {} (starred {})", star.session_ref, star.starred_at)?;
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

    fn star() -> Star {
        Star {
            session_ref: "claude-code/session".to_string(),
            starred_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn star_payload_surfaces_writer_errors() {
        let mut out = FailingWriter;
        let err = write_star_payload(&mut out, &star(), "starred", OutputMode::Human).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn star_list_surfaces_writer_errors() {
        let mut out = FailingWriter;
        let err = write_star_list(&mut out, &[star()], OutputMode::Json).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }
}
