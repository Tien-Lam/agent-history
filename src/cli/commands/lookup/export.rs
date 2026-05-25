use std::path::PathBuf;

use aghist::export;
use clap::Args;

use super::parse_reference_selector;

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
        required_unless_present = "params",
        value_parser = parse_reference_selector
    )]
    pub(crate) session: Option<String>,

    /// Output file path (defaults to stdout)
    #[arg(long, short, conflicts_with = "params")]
    pub(crate) output: Option<PathBuf>,

    /// Slice the session by 1-based turn range (e.g. `12:25`, `:10`, `5:`, or `7`).
    /// Bounds are inclusive. Out-of-range bounds clamp to the available messages.
    #[arg(long, conflicts_with = "params", value_parser = parse_turn_range_selector)]
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

fn parse_turn_range_selector(raw: &str) -> Result<String, String> {
    if raw.len() > aghist::schema_fragments::EXPORT_TURN_RANGE_MAX_BYTES {
        return Err(format!(
            "turn range must be at most {} bytes",
            aghist::schema_fragments::EXPORT_TURN_RANGE_MAX_BYTES
        ));
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aghist::schema_fragments::EXPORT_TURN_RANGE_MAX_BYTES;

    #[test]
    fn parse_turn_range_selector_rejects_oversized_values() {
        let raw = "1".repeat(EXPORT_TURN_RANGE_MAX_BYTES + 1);
        let err = parse_turn_range_selector(&raw).unwrap_err();
        assert!(err.contains(&EXPORT_TURN_RANGE_MAX_BYTES.to_string()));
    }
}
