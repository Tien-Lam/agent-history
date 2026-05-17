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

pub fn write_json_line<W, T>(out: &mut W, value: &T) -> io::Result<()>
where
    W: Write,
    T: Serialize + ?Sized,
{
    serde_json::to_writer(&mut *out, value).map_err(io::Error::other)?;
    writeln!(out)
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
}
