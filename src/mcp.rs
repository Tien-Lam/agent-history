//! Stdio MCP server: exposes aghist's read paths over JSON-RPC 2.0.
//!
//! Transport is newline-delimited JSON on stdin/stdout per the MCP stdio spec.
//! Logs go through `tracing` to a file (configured in `main`); nothing else
//! may touch stdout while the server is running.
//!
//! Tools exposed (see `tools/list`):
//! - `search_sessions` — full-text search, returns hits keyed by citation ref
//! - `list_sessions`   — provider-aware session listing
//! - `get_session`     — one session's metadata + bounded ordered turns
//! - `get_message`     — resolves `<provider>/<id>#<turn>` or
//!   `<source>:<provider>/<id>#<turn>`
//! - `reindex`         — incremental or `--force` rebuild of the search index
//! - `health`          — same checks as `aghist health`
//!
//! Resources exposed (see `resources/list` / `resources/read`):
//! - `aghist://session/<provider>/<session-id>` — session metadata + bounded turns
//! - `aghist://session/<provider>/<session-id>/turn/<n>` — single turn (1-based)
//! - `aghist://source/<source>/session/<provider>/<session-id>` — remote
//!   source session metadata + bounded turns
//! - `aghist://source/<source>/session/<provider>/<session-id>/turn/<n>` —
//!   single turn from a remote source
//!
//! Local URI shapes mirror the citation-ref triple. Remote sessions add an
//! explicit source segment so duplicate provider/session ids from different
//! machines remain round-trippable.
//!
//! ## Read-only contract
//!
//! No tool or resource exposed by this server may mutate provider history. The
//! `HistoryProvider` trait deliberately offers only read methods
//! (`discover_sessions`, `load_messages`) — there is no write surface to call.
//! Tool calls may rebuild the local Tantivy index (a derived cache under
//! `~/.aghist/`), but they never write back to the upstream session files.
//! Adding a tool that violates this contract requires loosening the trait,
//! which should be a deliberate design change — not a quiet edit here.
//!
//! ## Provider scoping
//!
//! `main` filters `config.enabled_providers()` further by
//! `config.mcp_exposed_providers()` so users can hide a provider from MCP
//! clients without disabling it for the TUI. That same visible-provider set is
//! applied to registered remote source caches. When the resulting list is
//! empty, every tool that walks providers returns an empty result rather than
//! erroring — the same behaviour as having no sessions discovered.

mod args;
mod payload;
mod protocol;
mod resource_handlers;
mod resources;
mod server;
mod tool_handlers;

#[cfg(test)]
mod tests;

pub use protocol::PROTOCOL_VERSION;
pub use server::McpServer;
