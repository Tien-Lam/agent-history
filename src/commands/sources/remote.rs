use std::io;
use std::path::{Path, PathBuf};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::config;
use aghist::output::{write_json_line, OutputMode};

mod pull;

pub(crate) use pull::sources_pull_remote;

pub(super) fn resolve_config_path() -> Result<PathBuf, ErrorEnvelope> {
    config::Config::resolved_path().ok_or_else(|| {
        ErrorEnvelope::new(
            "config-error",
            "could not determine config path; HOME and XDG_CONFIG_HOME are unset",
        )
        .with_hint("Set AGHIST_CONFIG=/path/to/config.toml to override.")
    })
}

fn write_sources_payload<W: io::Write>(
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

fn write_added_source<W: io::Write>(
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

fn write_removed_source<W: io::Write>(
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

pub(crate) fn sources_list_remote(mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let config_path = resolve_config_path()?;
    let config = config::Config::load_from(&config_path);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_sources_payload(&mut out, &config.sources, &config_path, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write sources output", e))?;
    if config.sources.is_empty() {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

pub(crate) fn sources_add_remote(
    name: &str,
    host: &str,
    path: &str,
    transport: config::Transport,
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    let trimmed_name = name.trim();
    if let Err(message) = config::validate_source_name(name) {
        return Err(ErrorEnvelope::new("usage", message)
            .with_hint("Pick a stable identifier, e.g. `laptop` or `prod-box`."));
    }
    if let Err(message) = config::validate_rsync_endpoint(host, "--host") {
        return Err(ErrorEnvelope::new("usage", message));
    }
    if let Err(message) = config::validate_rsync_endpoint(path, "--path") {
        return Err(ErrorEnvelope::new("usage", message));
    }

    let config_path = resolve_config_path()?;
    let mut config = config::Config::load_from(&config_path);
    if config.sources.iter().any(|s| s.name == trimmed_name) {
        return Err(ErrorEnvelope::new(
            "duplicate-source",
            format!("a source named '{trimmed_name}' already exists"),
        )
        .with_hint("Use `aghist sources remove <name>` first, or pick a different name."));
    }

    let new_source = config::RemoteSource {
        name: trimmed_name.to_string(),
        host: host.to_string(),
        path: path.to_string(),
        transport,
    };
    config.sources.push(new_source.clone());
    config.save_to(&config_path).map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to write {}: {e}", config_path.display()),
        )
    })?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_added_source(&mut out, &new_source, &config_path, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write sources output", e))?;
    Ok(EXIT_OK)
}

pub(crate) fn sources_remove_remote(name: &str, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let config_path = resolve_config_path()?;
    let mut config = config::Config::load_from(&config_path);
    let before = config.sources.len();
    let mut removed: Option<config::RemoteSource> = None;
    config.sources.retain(|s| {
        if s.name == name {
            removed = Some(s.clone());
            false
        } else {
            true
        }
    });
    if config.sources.len() == before {
        return Err(ErrorEnvelope::new(
            "source-not-found",
            format!("no registered source named '{name}'"),
        )
        .with_hint("Run `aghist sources list` to see registered sources."));
    }
    config.save_to(&config_path).map_err(|e| {
        ErrorEnvelope::new(
            "io-error",
            format!("failed to write {}: {e}", config_path.display()),
        )
    })?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let Some(removed) = removed else {
        return Err(ErrorEnvelope::new(
            "internal-error",
            "source removal changed the list without retaining the removed source",
        ));
    };
    write_removed_source(&mut out, &removed, &config_path, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write sources output", e))?;
    Ok(EXIT_OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingWriter;

    impl io::Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn source() -> config::RemoteSource {
        config::RemoteSource {
            name: "workstation".to_string(),
            host: "example.test".to_string(),
            path: "/home/me/.aghist".to_string(),
            transport: config::Transport::Ssh,
        }
    }

    #[test]
    fn added_source_surfaces_writer_errors() {
        let mut out = FailingWriter;
        let err = write_added_source(
            &mut out,
            &source(),
            Path::new("/tmp/config.toml"),
            OutputMode::Human,
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn removed_source_surfaces_writer_errors() {
        let mut out = FailingWriter;
        let err = write_removed_source(
            &mut out,
            &source(),
            Path::new("/tmp/config.toml"),
            OutputMode::Json,
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }
}
