use std::fmt;

use crate::cli_error::ErrorEnvelope;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionError {
    InvalidSourceName(String),
    InvalidSessionRef {
        selector: String,
        message: String,
    },
    InvalidCitationRef {
        selector: String,
        message: String,
    },
    TurnRefForSession,
    SessionRefRequired(String),
    NotFound(String),
    Ambiguous {
        selector: String,
        count: usize,
        candidates: Vec<String>,
    },
}

impl ResolutionError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidSourceName(_)
            | Self::InvalidSessionRef { .. }
            | Self::InvalidCitationRef { .. }
            | Self::TurnRefForSession
            | Self::SessionRefRequired(_) => "usage",
            Self::NotFound(_) => "session-not-found",
            Self::Ambiguous { .. } => "ambiguous-session",
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::InvalidSessionRef { .. } => Some(
                "Format: <provider-slug>/<session-id> or <source>:<provider-slug>/<session-id>.",
            ),
            Self::InvalidCitationRef { .. } => Some(
                "Format: <provider-slug>/<session-id>#<turn> or <source>:<provider-slug>/<session-id>#<turn>.",
            ),
            Self::TurnRefForSession => Some(
                "Use `aghist show <ref>` for a single turn, or remove the `#<turn>` suffix.",
            ),
            Self::SessionRefRequired(_) => {
                Some("Use <source>:<provider>/<session-id> for remote-source sessions.")
            }
            Self::NotFound(_) => {
                Some("Run `aghist --list --json` to see available sessions and sources.")
            }
            Self::Ambiguous { .. } => {
                Some("Use <source>:<provider>/<session-id> to choose one session explicitly.")
            }
            Self::InvalidSourceName(_) => None,
        }
    }

    pub fn into_error_envelope(self) -> ErrorEnvelope {
        let hint = self.hint();
        let mut envelope = ErrorEnvelope::new(self.kind(), self.to_string());
        if let Some(hint) = hint {
            envelope = envelope.with_hint(hint);
        }
        envelope
    }
}

impl fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSourceName(message) => f.write_str(message),
            Self::InvalidSessionRef { selector, message } => {
                write!(f, "invalid session ref '{selector}': {message}")
            }
            Self::InvalidCitationRef { selector, message } => {
                write!(f, "invalid citation ref '{selector}': {message}")
            }
            Self::TurnRefForSession => {
                f.write_str("expected a session ref, not a turn-level citation ref")
            }
            Self::SessionRefRequired(selector) => {
                write!(
                    f,
                    "invalid session ref '{selector}': expected <provider>/<session-id>"
                )
            }
            Self::NotFound(selector) => write!(f, "Session not found: {selector}"),
            Self::Ambiguous {
                selector,
                count,
                candidates,
            } => write!(
                f,
                "session selector '{selector}' matched {count} sessions: {}",
                candidates.join(", ")
            ),
        }
    }
}

impl From<ResolutionError> for ErrorEnvelope {
    fn from(value: ResolutionError) -> Self {
        value.into_error_envelope()
    }
}
