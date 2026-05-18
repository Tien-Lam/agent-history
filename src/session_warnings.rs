use std::fmt;

use crate::federated::{SourceError, LOCAL_SOURCE};
use crate::model::Session;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLoadWarning {
    pub source: String,
    pub session_ref: String,
    pub error: String,
}

impl SessionLoadWarning {
    pub fn new(source: &str, session: &Session, error: impl fmt::Display) -> Self {
        Self {
            source: source.to_string(),
            session_ref: qualified_session_ref(source, session),
            error: error.to_string(),
        }
    }

    pub fn warning_line(&self) -> String {
        format!(
            "warning: skipped session '{}': failed to load messages: {}",
            self.session_ref, self.error
        )
    }

    pub fn source_error(&self) -> SourceError {
        SourceError {
            source: self.source.clone(),
            error: format!(
                "skipped session '{}': failed to load messages: {}",
                self.session_ref, self.error
            ),
        }
    }
}

fn qualified_session_ref(source: &str, session: &Session) -> String {
    let raw = session.session_ref().to_string();
    if source == LOCAL_SOURCE {
        raw
    } else {
        format!("{source}:{raw}")
    }
}
