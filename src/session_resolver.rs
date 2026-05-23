mod error;
mod parse;
mod refs;
mod resolver;
mod source;
mod types;

pub use error::ResolutionError;
pub use refs::{
    qualified_citation_ref, qualified_session_metadata_key, qualified_session_ref,
    session_metadata_key, source_for_session,
};
pub use resolver::SessionResolver;
pub use source::LookupSource;
pub use types::{SelectedCitation, SelectedSession, SelectorShape};

#[cfg(test)]
mod tests;
