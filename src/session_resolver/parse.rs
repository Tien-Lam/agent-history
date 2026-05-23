use crate::model::{CitationRef, SessionRef};

use super::ResolutionError;

pub(super) fn parse_session_ref(
    raw: &str,
    full_selector: &str,
) -> Result<SessionRef, ResolutionError> {
    raw.parse::<SessionRef>()
        .map_err(|e| ResolutionError::InvalidSessionRef {
            selector: full_selector.to_string(),
            message: e.to_string(),
        })
}

pub(super) fn parse_citation_ref(
    raw: &str,
    full_selector: &str,
) -> Result<CitationRef, ResolutionError> {
    raw.parse::<CitationRef>()
        .map_err(|e| ResolutionError::InvalidCitationRef {
            selector: full_selector.to_string(),
            message: e.to_string(),
        })
}
