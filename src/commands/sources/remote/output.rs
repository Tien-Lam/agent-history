use std::io;
use std::path::Path;

use aghist::config;
use aghist::output::{write_json_line, OutputMode};

pub(crate) fn write_sources_payload<W: io::Write>(
    out: &mut W,
    sources: &[config::RemoteSource],
    config_path: &Path,
    mode: OutputMode,
) -> io::Result<()> {
    match mode {
        OutputMode::Human => render_remote_sources_human(out, sources, config_path),
        OutputMode::Json => {
            let payload = serde_json::json!({
                "sources": sources,
                "config_path": config_path.display().to_string(),
            });
            write_json_line(out, &payload)
        }
        OutputMode::Ndjson => {
            for s in sources {
                write_json_line(out, s)?;
            }
            Ok(())
        }
    }
}

pub(crate) fn write_added_source<W: io::Write>(
    out: &mut W,
    source: &config::RemoteSource,
    config_path: &Path,
    mode: OutputMode,
) -> io::Result<()> {
    if mode.is_machine() {
        let payload = serde_json::json!({
            "added": source,
            "config_path": config_path.display().to_string(),
        });
        write_json_line(out, &payload)
    } else {
        writeln!(
            out,
            "Added source '{}' ({} {}:{})",
            source.name,
            source.transport.slug(),
            source.host,
            source.path
        )?;
        writeln!(out, "Config: {}", config_path.display())
    }
}

pub(crate) fn write_removed_source<W: io::Write>(
    out: &mut W,
    source: &config::RemoteSource,
    config_path: &Path,
    mode: OutputMode,
) -> io::Result<()> {
    if mode.is_machine() {
        let payload = serde_json::json!({
            "removed": source,
            "config_path": config_path.display().to_string(),
        });
        write_json_line(out, &payload)
    } else {
        writeln!(out, "Removed source '{}'", source.name)?;
        writeln!(out, "Config: {}", config_path.display())
    }
}

fn render_remote_sources_human<W: io::Write>(
    out: &mut W,
    sources: &[config::RemoteSource],
    config_path: &Path,
) -> io::Result<()> {
    if sources.is_empty() {
        writeln!(
            out,
            "No remote sources registered. Add one with `aghist sources add <name> --host <host> --path <path>`."
        )?;
        writeln!(out, "Config: {}", config_path.display())?;
        return Ok(());
    }
    writeln!(
        out,
        "{:<20}  {:<10}  {:<25}  PATH",
        "NAME", "TRANSPORT", "HOST"
    )?;
    for s in sources {
        writeln!(
            out,
            "{:<20}  {:<10}  {:<25}  {}",
            s.name,
            s.transport.slug(),
            s.host,
            s.path
        )?;
    }
    writeln!(out)?;
    writeln!(out, "Config: {}", config_path.display())?;
    Ok(())
}
