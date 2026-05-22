use std::io;
use std::path::{Path, PathBuf};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::config;
use aghist::output::{write_json_line, OutputMode};
use aghist::services::source_registry::{self, SourceRegistryError};

mod pull;

pub(crate) use pull::sources_pull_remote;

pub(super) fn resolve_config_path() -> Result<PathBuf, ErrorEnvelope> {
    source_registry::resolve_config_path().map_err(registry_error_to_envelope)
}

pub(super) fn load_sources_config(config_path: &Path) -> Result<config::Config, ErrorEnvelope> {
    source_registry::load_config(config_path).map_err(registry_error_to_envelope)
}

fn registry_error_to_envelope(error: SourceRegistryError) -> ErrorEnvelope {
    match error {
        SourceRegistryError::ConfigPathUnavailable => ErrorEnvelope::new(
            "config-error",
            "could not determine config path; HOME and XDG_CONFIG_HOME are unset",
        )
        .with_hint("Set AGHIST_CONFIG=/path/to/config.toml to override."),
        SourceRegistryError::ConfigLoad(error) => {
            ErrorEnvelope::new("config-error", format!("{error}"))
                .with_hint("Fix the TOML before changing the remote-source registry.")
        }
        SourceRegistryError::InvalidName(message) => ErrorEnvelope::new("usage", message)
            .with_hint("Pick a stable identifier, e.g. `laptop` or `prod-box`."),
        SourceRegistryError::InvalidHost(message) | SourceRegistryError::InvalidPath(message) => {
            ErrorEnvelope::new("usage", message)
        }
        SourceRegistryError::DuplicateSource(name) => ErrorEnvelope::new(
            "duplicate-source",
            format!("a source named '{name}' already exists"),
        )
        .with_hint("Use `aghist sources remove <name>` first, or pick a different name."),
        SourceRegistryError::SourceNotFound(name) => ErrorEnvelope::new(
            "source-not-found",
            format!("no registered source named '{name}'"),
        )
        .with_hint("Run `aghist sources list` to see registered sources."),
        SourceRegistryError::Save { path, source } => ErrorEnvelope::new(
            "io-error",
            format!("failed to write {}: {source}", path.display()),
        ),
    }
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
    let sources =
        source_registry::list_remote_sources(&config_path).map_err(registry_error_to_envelope)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_sources_payload(&mut out, &sources, &config_path, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write sources output", e))?;
    if sources.is_empty() {
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
    let config_path = resolve_config_path()?;
    let new_source = source_registry::add_remote_source(&config_path, name, host, path, transport)
        .map_err(registry_error_to_envelope)?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_added_source(&mut out, &new_source, &config_path, mode)
        .map_err(|e| ErrorEnvelope::io("failed to write sources output", e))?;
    Ok(EXIT_OK)
}

pub(crate) fn sources_remove_remote(name: &str, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let config_path = resolve_config_path()?;
    let removed = source_registry::remove_remote_source(&config_path, name)
        .map_err(registry_error_to_envelope)?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
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

    #[test]
    fn sources_config_loader_surfaces_parse_errors() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        std::fs::write(&config_path, "not = [valid").unwrap();

        let err = load_sources_config(&config_path).unwrap_err();

        assert_eq!(err.kind, "config-error");
        assert!(err.message.contains("failed to parse"), "{}", err.message);
    }
}
