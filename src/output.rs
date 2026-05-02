use std::io::IsTerminal;

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
