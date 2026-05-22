//! Stable JSON error envelope and semantic exit codes for the `aghist` CLI.
//!
//! On error, the binary writes one line of JSON to stderr in the shape
//! `{"error":{"kind":"<kebab>","message":"...","hint":"..."}}` and exits with
//! a semantic code:
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0    | success with results |
//! | 1    | runtime error (envelope written to stderr) |
//! | 2    | usage error (bad flags, parse failure) |
//! | 3    | success but empty (no rows / no hits) |
//!
//! The canonical list of `kind` values lives in `AGENTS.md`.

use std::fmt;
use std::io::{self, Write};

use serde::Serialize;

pub const EXIT_OK: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_EMPTY: i32 = 3;

#[derive(Debug, Clone, Serialize)]
pub struct ErrorEnvelope {
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Serialize)]
struct Wrapper<'a> {
    error: &'a ErrorEnvelope,
}

impl ErrorEnvelope {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            hint: None,
        }
    }

    pub fn io(action: impl AsRef<str>, error: impl fmt::Display) -> Self {
        Self::new("io-error", format!("{}: {error}", action.as_ref()))
    }

    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Serialize as a single JSON line to stderr. Best-effort: a failure to
    /// write the envelope is silently ignored — the process still exits with
    /// the appropriate code.
    pub fn emit(&self) {
        let wrapped = Wrapper { error: self };
        if let Ok(json) = serde_json::to_string(&wrapped) {
            let mut stderr = io::stderr().lock();
            let _ = writeln!(stderr, "{json}");
        }
    }

    pub fn to_json_string(&self) -> String {
        let wrapped = Wrapper { error: self };
        serde_json::to_string(&wrapped).unwrap_or_else(|_| {
            String::from(
                r#"{"error":{"kind":"internal-error","message":"failed to serialize envelope"}}"#,
            )
        })
    }

    pub fn exit_code(&self) -> i32 {
        if self.kind == "usage" {
            EXIT_USAGE
        } else {
            EXIT_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_serializes_with_hint() {
        let env = ErrorEnvelope::new("session-not-found", "no such session: abc")
            .with_hint("Run `aghist --list` to see valid IDs.");
        let json = env.to_json_string();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["error"]["kind"], "session-not-found");
        assert_eq!(parsed["error"]["message"], "no such session: abc");
        assert_eq!(
            parsed["error"]["hint"],
            "Run `aghist --list` to see valid IDs."
        );
    }

    #[test]
    fn envelope_serializes_without_hint() {
        let env = ErrorEnvelope::new("io-error", "permission denied");
        let json = env.to_json_string();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["error"]["kind"], "io-error");
        assert_eq!(parsed["error"]["message"], "permission denied");
        assert!(parsed["error"].get("hint").is_none());
    }

    #[test]
    fn exit_codes_are_stable() {
        assert_eq!(EXIT_OK, 0);
        assert_eq!(EXIT_ERROR, 1);
        assert_eq!(EXIT_USAGE, 2);
        assert_eq!(EXIT_EMPTY, 3);
    }

    #[test]
    fn usage_envelopes_exit_two() {
        assert_eq!(ErrorEnvelope::new("usage", "bad flag").exit_code(), 2);
        assert_eq!(
            ErrorEnvelope::new("session-not-found", "missing").exit_code(),
            1
        );
    }
}
