use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::Provider;
use aghist::output::{write_json_line, OutputMode};
use aghist::{provider, search};

mod remote;

pub(crate) use remote::{
    sources_add_remote, sources_list_remote, sources_pull_remote, sources_remove_remote,
};

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
}

fn collect_source_row(p: &dyn provider::HistoryProvider) -> SourceRow {
    let mut paths = Vec::new();
    let mut total_bytes: u64 = 0;
    for dir in p.base_dirs() {
        let exists = dir.exists();
        let bytes = if exists { dir_size_bytes(dir) } else { 0 };
        total_bytes = total_bytes.saturating_add(bytes);
        paths.push(SourcePath {
            path: dir.display().to_string(),
            exists,
            bytes,
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

/// Recursive directory size in bytes. Symlinks and IO errors are skipped.
fn dir_size_bytes(dir: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_file() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            total = total.saturating_add(meta.len());
        } else if file_type.is_dir() {
            total = total.saturating_add(dir_size_bytes(&entry.path()));
        }
    }
    total
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
                if p.exists {
                    p.path.clone()
                } else {
                    format!("{} (missing)", p.path)
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

#[allow(clippy::cast_precision_loss)]
fn format_bytes(b: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if b >= GB {
        format!("{:.1}G", b as f64 / GB as f64)
    } else if b >= MB {
        format!("{:.1}M", b as f64 / MB as f64)
    } else if b >= KB {
        format!("{:.1}K", b as f64 / KB as f64)
    } else {
        format!("{b}B")
    }
}
