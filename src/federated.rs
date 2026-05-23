//! Federated session discovery across local providers and remote source caches.
//!
//! Local provider directories (e.g. `~/.claude`, `~/.codex`) and remote source
//! caches under `<sources_cache>/<name>/data/` are scanned together. Each
//! scan runs in its own thread (concurrent fanout); a failing source is
//! recorded but does not abort the others (partial-failure tolerant).
//!
//! The result tags every session with the source it came from — `"local"` for
//! the host running aghist, or the registered source name for remote mirrors —
//! so search results can surface a `source` marker. Callers consult
//! [`FederatedDiscovery::source_of_session`] to map a session back to its
//! source.

mod discovery;
mod types;

pub use discovery::{discover_federated, discover_remote_sources, providers_rooted_at};
pub use types::{source_errors, FederatedDiscovery, SourceError, SourceFailure};

/// Source tag for sessions discovered from local provider dirs.
pub const LOCAL_SOURCE: &str = "local";

#[cfg(test)]
mod tests;
