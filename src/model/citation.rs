//! Stable citation references for individual messages within a session.
//!
//! A [`SessionRef`] identifies a session via `(provider, session_id)`,
//! formatted as `<provider-slug>/<session-id>`.
//!
//! A [`CitationRef`] identifies one message via the triple `(provider,
//! session_id, turn)`, formatted as `<provider-slug>/<session-id>#<turn>`
//! (e.g. `claude-code/abc-123#7`).
//!
//! Refs are designed to be:
//! - **Opaque-stable across reindex**: rebuilding the search index does not
//!   change the ref for a given message. The provider slug and session id
//!   are intrinsic to the source data; the turn is the 1-based index of the
//!   message within the session in load order.
//! - **Round-trippable**: `parse(format(r)) == r` for every well-formed ref.
//! - **Human-quotable**: the form fits inline in chat / docs / commit
//!   messages without escaping.
//!
//! Turns are 1-based. Turn `0` is rejected at parse time.

mod base;
mod qualified;

pub use base::{CitationParseError, CitationRef, SessionOrTurnRef, SessionRef};
pub use qualified::{split_source_prefix, QualifiedCitationRef};

#[cfg(test)]
mod tests;
