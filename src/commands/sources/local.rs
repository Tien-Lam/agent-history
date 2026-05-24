use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::Provider;
use aghist::output::{write_json_line, OutputMode};
use aghist::{provider, search};

use super::{dir_accounting::dir_size_bytes, format_bytes};

pub(crate) fn sources_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    let index_dir = search::SearchIndex::default_index_dir();
    let manifest_path = index_dir.join("manifest.json");
    let last_indexed_at = std::fs::metadata(&manifest_path)
        .and_then(|m| m.modified())
        .ok()
        .map(chrono::DateTime::<chrono::Utc>::from);

    let rows: Vec<SourceRow> = providers
        .iter()
        .map(|p| collect_source_row(p.as_ref()))
        .collect();

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Human => render_sources_human(&mut out, &rows, &index_dir, last_indexed_at),
        OutputMode::Json => render_sources_json(&mut out, &rows, &index_dir, last_indexed_at),
        OutputMode::Ndjson => render_sources_ndjson(&mut out, &rows),
    }
    .map_err(|e| ErrorEnvelope::io("failed to write sources output", e))?;

    if rows.is_empty() {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

#[derive(serde::Serialize)]
struct SourceRow {
    provider: Provider,
    paths: Vec<SourcePath>,
    session_count: usize,
    total_bytes: u64,
    discover_error: Option<String>,
}

#[derive(serde::Serialize)]
struct SourcePath {
    path: String,
    exists: bool,
    bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_error: Option<String>,
}

fn collect_source_row(p: &dyn provider::HistoryProvider) -> SourceRow {
    let mut paths = Vec::new();
    let mut total_bytes: u64 = 0;
    for dir in p.base_dirs() {
        let exists = dir.exists();
        let (bytes, size_error) = if exists {
            match dir_size_bytes(dir) {
                Ok(bytes) => (bytes, None),
                Err(err) => (0, Some(err.to_string())),
            }
        } else {
            (0, None)
        };
        total_bytes = total_bytes.saturating_add(bytes);
        paths.push(SourcePath {
            path: dir.display().to_string(),
            exists,
            bytes,
            size_error,
        });
    }

    let (session_count, discover_error) = match p.discover_sessions() {
        Ok(s) => (s.len(), None),
        Err(e) => (0, Some(e.to_string())),
    };

    SourceRow {
        provider: p.provider(),
        paths,
        session_count,
        total_bytes,
        discover_error,
    }
}

fn render_sources_human<W: io::Write>(
    out: &mut W,
    rows: &[SourceRow],
    index_dir: &std::path::Path,
    last_indexed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> io::Result<()> {
    if rows.is_empty() {
        writeln!(
            out,
            "No providers detected. Check your config (`providers` table)."
        )?;
        return Ok(());
    }
    writeln!(
        out,
        "{:<14}  {:<8}  {:<10}  PATHS",
        "PROVIDER", "SESSIONS", "SIZE"
    )?;
    for row in rows {
        let paths_str = row
            .paths
            .iter()
            .map(|p| {
                if let Some(err) = &p.size_error {
                    format!("{} (size unavailable: {err})", p.path)
                } else if !p.exists {
                    format!("{} (missing)", p.path)
                } else {
                    p.path.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let size = format_bytes(row.total_bytes);
        writeln!(
            out,
            "{:<14}  {:<8}  {:<10}  {paths_str}",
            row.provider.slug(),
            row.session_count,
            size
        )?;
        if let Some(err) = &row.discover_error {
            writeln!(out, "  ! discover error: {err}")?;
        }
    }
    writeln!(out)?;
    writeln!(out, "Index dir: {}", index_dir.display())?;
    if let Some(ts) = last_indexed_at {
        writeln!(out, "Last indexed: {}", ts.format("%Y-%m-%d %H:%M UTC"))?;
    } else {
        writeln!(out, "Last indexed: never (run `aghist index`)")?;
    }
    Ok(())
}

fn render_sources_json<W: io::Write>(
    out: &mut W,
    rows: &[SourceRow],
    index_dir: &std::path::Path,
    last_indexed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> io::Result<()> {
    let payload = serde_json::json!({
        "sources": rows,
        "index": {
            "dir": index_dir.display().to_string(),
            "last_indexed_at": last_indexed_at,
        },
    });
    write_json_line(out, &payload)
}

fn render_sources_ndjson<W: io::Write>(out: &mut W, rows: &[SourceRow]) -> io::Result<()> {
    for row in rows {
        write_json_line(out, row)?;
    }
    Ok(())
}
