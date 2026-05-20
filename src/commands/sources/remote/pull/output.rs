use std::io;
use std::path::Path;

use aghist::output::{write_json_line, OutputMode};
use chrono::{DateTime, Utc};

use super::super::super::format_bytes;

#[derive(serde::Serialize)]
pub(super) struct PullResult {
    pub(super) name: String,
    pub(super) host: String,
    pub(super) path: String,
    pub(super) transport: String,
    pub(super) data_dir: String,
    pub(super) dry_run: bool,
    pub(super) byte_count: u64,
    pub(super) file_count: u64,
    pub(super) pulled_at: DateTime<Utc>,
}

pub(super) fn write_pull_results<W: io::Write>(
    out: &mut W,
    results: &[PullResult],
    cache_root: &Path,
    mode: OutputMode,
) -> io::Result<()> {
    match mode {
        OutputMode::Human => {
            if results.is_empty() {
                writeln!(out, "No sources pulled.")?;
                return Ok(());
            }
            writeln!(
                out,
                "{:<20}  {:<8}  {:<10}  {:<6}  PATH",
                "NAME", "FILES", "SIZE", "DRY"
            )?;
            for r in results {
                writeln!(
                    out,
                    "{:<20}  {:<8}  {:<10}  {:<6}  {}",
                    r.name,
                    r.file_count,
                    format_bytes(r.byte_count),
                    if r.dry_run { "yes" } else { "no" },
                    r.data_dir
                )?;
            }
            writeln!(out)?;
            writeln!(out, "Cache: {}", cache_root.display())?;
            Ok(())
        }
        OutputMode::Json => {
            let payload = serde_json::json!({
                "results": results,
                "cache_dir": cache_root.display().to_string(),
            });
            write_json_line(out, &payload)
        }
        OutputMode::Ndjson => {
            for r in results {
                write_json_line(out, r)?;
            }
            Ok(())
        }
    }
}
