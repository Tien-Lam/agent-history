use std::path::PathBuf;

use aghist::export;
use clap::Args;

#[derive(Args)]
pub(crate) struct ExportCommand {
    /// Output format: md, json, html
    #[arg(
        long,
        short,
        conflicts_with = "params",
        required_unless_present = "params"
    )]
    pub(crate) format: Option<export::ExportFormat>,

    /// Session ID/prefix, `<provider>/<id>`, or `<source>:<provider>/<id>` to export
    #[arg(
        long,
        short,
        conflicts_with = "params",
        required_unless_present = "params"
    )]
    pub(crate) session: Option<String>,

    /// Output file path (defaults to stdout)
    #[arg(long, short, conflicts_with = "params")]
    pub(crate) output: Option<PathBuf>,

    /// Slice the session by 1-based turn range (e.g. `12:25`, `:10`, `5:`, or `7`).
    /// Bounds are inclusive. Out-of-range bounds clamp to the available messages.
    #[arg(long, conflicts_with = "params")]
    pub(crate) turn_range: Option<String>,

    /// Inline private annotations (notes from the metadata sidecar) at their
    /// citation refs. Session-level notes render once near the top; turn-level
    /// notes render after the message they're attached to. Notes stay marked
    /// "private annotation" so consumers don't conflate them with session
    /// content. No-op when the metadata sidecar is absent or has no matching
    /// notes.
    #[arg(long, conflicts_with = "params")]
    pub(crate) include_notes: bool,

    /// JSON request body containing all params at once. Mutually exclusive
    /// with other flags. Schema: `{format, session, output?, turn_range?, include_notes?}`.
    /// Lets agents skip per-flag discovery and submit a single JSON request.
    #[arg(long, value_name = "JSON")]
    pub(crate) params: Option<String>,
}
