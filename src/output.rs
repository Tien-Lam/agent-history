use std::io::{self, IsTerminal, Write};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Json,
    Ndjson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Streaming,
    OneShot,
}

impl OutputMode {
    pub fn resolve(json: bool, ndjson: bool, kind: CommandKind) -> Self {
        if ndjson {
            return Self::Ndjson;
        }
        if json {
            return Self::Json;
        }
        if std::io::stdout().is_terminal() {
            Self::Human
        } else {
            match kind {
                CommandKind::Streaming => Self::Ndjson,
                CommandKind::OneShot => Self::Json,
            }
        }
    }

    pub fn is_machine(self) -> bool {
        matches!(self, Self::Json | Self::Ndjson)
    }
}

pub fn should_emit_json(force_json: bool) -> bool {
    force_json || !io::stdout().is_terminal()
}

pub fn write_json_line<W, T>(out: &mut W, value: &T) -> io::Result<()>
where
    W: Write,
    T: Serialize + ?Sized,
{
    serde_json::to_writer(&mut *out, value).map_err(json_to_io_error)?;
    writeln!(out)
}

fn json_to_io_error(error: serde_json::Error) -> io::Error {
    if let Some(kind) = error.io_error_kind() {
        io::Error::new(kind, error)
    } else {
        io::Error::other(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_json_wins() {
        assert_eq!(
            OutputMode::resolve(true, false, CommandKind::Streaming),
            OutputMode::Json
        );
    }

    #[test]
    fn explicit_ndjson_wins() {
        assert_eq!(
            OutputMode::resolve(false, true, CommandKind::OneShot),
            OutputMode::Ndjson
        );
    }

    #[test]
    fn ndjson_takes_precedence_over_json() {
        assert_eq!(
            OutputMode::resolve(true, true, CommandKind::OneShot),
            OutputMode::Ndjson
        );
    }

    #[test]
    fn write_json_line_preserves_writer_error_kind() {
        struct BrokenWriter;

        impl Write for BrokenWriter {
            fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut out = BrokenWriter;
        let err = write_json_line(&mut out, &serde_json::json!({"ok": true})).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }
}
