mod error;
mod refs;
mod resolver;
mod source;

pub use error::ResolutionError;
pub use refs::{
    qualified_citation_ref, qualified_session_metadata_key, qualified_session_ref,
    session_metadata_key, source_for_session,
};
pub use resolver::{SelectedCitation, SelectedSession, SelectorShape, SessionResolver};
pub use source::LookupSource;

#[cfg(test)]
mod tests;
