use std::io;

use aghist::cli_error::ErrorEnvelope;
use aghist::metadata::Tag;
use aghist::output::{write_json_line, OutputMode};

pub(super) fn emit_tag_payload(
    tag: &Tag,
    action: &str,
    mode: OutputMode,
) -> Result<(), ErrorEnvelope> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_tag_payload(&mut out, tag, action, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write tag output", e))
}

fn write_tag_payload<W: io::Write>(
    out: &mut W,
    tag: &Tag,
    action: &str,
    mode: OutputMode,
) -> io::Result<()> {
    if mode.is_machine() {
        let payload = serde_json::json!({ action: tag });
        write_json_line(out, &payload)?;
    } else {
        writeln!(out, "{action} tag '{}' on {}", tag.tag, tag.session_ref)?;
    }
    Ok(())
}

pub(super) fn emit_tag_list(tags: &[Tag], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_tag_list(&mut out, tags, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write tag output", e))
}

fn write_tag_list<W: io::Write>(out: &mut W, tags: &[Tag], mode: OutputMode) -> io::Result<()> {
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "tags": tags, "count": tags.len() });
            write_json_line(out, &payload)?;
        }
        OutputMode::Ndjson => {
            for tag in tags {
                write_json_line(out, tag)?;
            }
        }
        OutputMode::Human => {
            if tags.is_empty() {
                writeln!(out, "(no tags)")?;
            } else {
                for tag in tags {
                    writeln!(
                        out,
                        "#{} {} [{}] (created {})",
                        tag.id, tag.session_ref, tag.tag, tag.created_at
                    )?;
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

    fn tag() -> Tag {
        Tag {
            id: 3,
            session_ref: "claude-code/session".to_string(),
            tag: "review".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn tag_payload_surfaces_writer_errors() {
        let mut out = FailingWriter;
        let err = write_tag_payload(&mut out, &tag(), "added", OutputMode::Human).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn tag_list_surfaces_writer_errors() {
        let mut out = FailingWriter;
        let err = write_tag_list(&mut out, &[tag()], OutputMode::Ndjson).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }
}
