use std::path::PathBuf;

use aghist::config;
use aghist::schema_fragments::{METADATA_TAG_MAX_BYTES, REFERENCE_MAX_BYTES};
use clap::Subcommand;

use super::super::resolvers::parse_transport;

/// Subcommands of `aghist sources` that manage the remote-source registry.
#[derive(Subcommand)]
pub(crate) enum SourcesCommand {
    /// Register a new remote source. Persists to `config.toml`.
    Add {
        /// Stable identifier for the source (used by `remove`).
        #[arg(value_name = "NAME", value_parser = parse_source_name)]
        name: String,
        /// Hostname or `user@host` pointing at the remote machine.
        #[arg(long, value_name = "HOST", value_parser = parse_rsync_host)]
        host: String,
        /// Path on the remote machine where the agent history lives.
        #[arg(long, value_name = "PATH", value_parser = parse_rsync_path)]
        path: String,
        /// Transport used to reach the remote (`ssh` or `rsync`). Defaults to `ssh`.
        #[arg(long, default_value = "ssh", value_parser = parse_transport, value_name = "TRANSPORT")]
        transport: config::Transport,
    },
    /// List registered remote sources.
    List,
    /// Remove a registered remote source by name.
    Remove {
        /// Name of the source to remove (matches `add --name`).
        #[arg(value_name = "NAME", value_parser = parse_source_name)]
        name: String,
    },
    /// Pull a remote source's history into a local cache via rsync.
    ///
    /// Mirrors `<host>:<path>/` to `<cache>/sources/<name>/data/` using
    /// rsync. The cache root defaults to the platform cache dir (overridable
    /// via `AGHIST_SOURCES_CACHE_DIR`). After pulling, writes a per-source
    /// manifest with byte/file counts and the pull timestamp; downstream
    /// indexing and federated search consume these.
    ///
    /// `--all` pulls every registered source in turn. Pass `--dry-run` to
    /// invoke rsync with `--dry-run` (no files written) - useful to validate
    /// connectivity without mutating the cache. The rsync binary can be
    /// overridden with `AGHIST_RSYNC_BIN` (used by tests; not for end users).
    Pull {
        /// Name of the source to pull. Mutually exclusive with `--all`.
        #[arg(value_name = "NAME", conflicts_with = "all", value_parser = parse_source_name)]
        name: Option<String>,

        /// Pull every registered source.
        #[arg(long, conflicts_with = "name")]
        all: bool,

        /// Run rsync with `--dry-run`; no files are written.
        #[arg(long)]
        dry_run: bool,
    },
}

fn parse_source_name(raw: &str) -> Result<String, String> {
    config::validate_source_name(raw)?;
    Ok(raw.to_string())
}

fn parse_rsync_host(raw: &str) -> Result<String, String> {
    config::validate_rsync_host(raw, "--host")?;
    Ok(raw.to_string())
}

fn parse_rsync_path(raw: &str) -> Result<String, String> {
    config::validate_rsync_path(raw, "--path")?;
    Ok(raw.to_string())
}

/// Subcommands of `aghist note` that manage per-user session annotations.
#[derive(Subcommand)]
pub(crate) enum NoteCommand {
    /// Attach a new note to a session ref. The body is read from `--body`,
    /// `--body-file`, or stdin (`--stdin`). Outputs the created note as JSON
    /// (single object) on stdout.
    Add {
        /// Session ref: `<provider>/<session-id>[#<turn>]` or
        /// `<source>:<provider>/<session-id>[#<turn>]`.
        #[arg(value_name = "REF", value_parser = parse_metadata_reference)]
        reference: String,

        /// Note body as a literal string. Mutually exclusive with `--body-file`/`--stdin`.
        #[arg(long, conflicts_with_all = ["body_file", "stdin"], value_name = "TEXT")]
        body: Option<String>,

        /// Read the body from a file (use `-` for stdin).
        #[arg(long, conflicts_with_all = ["body", "stdin"], value_name = "PATH")]
        body_file: Option<PathBuf>,

        /// Read the body from standard input (read until EOF).
        #[arg(long, conflicts_with_all = ["body", "body_file"])]
        stdin: bool,
    },
    /// List notes, optionally filtered by session ref.
    ///
    /// With no ref: every note, newest first. With a session ref:
    /// every note on that session and any of its turns. With a turn-level ref:
    /// only notes on that exact turn. Remote sessions use a `<source>:` prefix.
    List {
        /// Optional session ref filter.
        #[arg(value_name = "REF", value_parser = parse_metadata_reference)]
        reference: Option<String>,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,
    },
    /// Replace the body of an existing note.
    Edit {
        /// Numeric note id (from `aghist note add` or `aghist note list`).
        #[arg(value_name = "ID", value_parser = parse_note_id, allow_hyphen_values = true)]
        id: i64,

        /// New body as a literal string. Mutually exclusive with `--body-file`/`--stdin`.
        #[arg(long, conflicts_with_all = ["body_file", "stdin"], value_name = "TEXT")]
        body: Option<String>,

        /// Read the new body from a file (use `-` for stdin).
        #[arg(long, conflicts_with_all = ["body", "stdin"], value_name = "PATH")]
        body_file: Option<PathBuf>,

        /// Read the new body from standard input (read until EOF).
        #[arg(long, conflicts_with_all = ["body", "body_file"])]
        stdin: bool,
    },
    /// Remove a note by id. Outputs the deleted row as JSON on stdout.
    #[command(alias = "rm")]
    Remove {
        /// Numeric note id.
        #[arg(value_name = "ID", value_parser = parse_note_id, allow_hyphen_values = true)]
        id: i64,
    },
}

/// Subcommands of `aghist tag` that manage per-user session tags.
#[derive(Subcommand)]
pub(crate) enum TagCommand {
    /// Attach a tag to a session ref. Outputs the created row as JSON on stdout.
    /// Adding the same (ref, tag) pair twice raises a `tag-conflict` error.
    Add {
        /// Session ref: `<provider>/<session-id>[#<turn>]` or
        /// `<source>:<provider>/<session-id>[#<turn>]`.
        #[arg(value_name = "REF", value_parser = parse_metadata_reference)]
        reference: String,

        /// Tag label. Whitespace-trimmed; must be non-empty.
        #[arg(value_name = "TAG", value_parser = parse_metadata_tag)]
        tag: String,
    },
    /// List tags, optionally filtered by session ref and/or tag value.
    ///
    /// With no arguments: every tag, newest first. With a session ref:
    /// every tag on that session and any of its turns. With a turn-level ref:
    /// tags on that exact turn. Remote sessions use a `<source>:` prefix.
    /// `--tag <name>` narrows to a specific tag value (combinable with the ref filter).
    List {
        /// Optional session ref filter.
        #[arg(value_name = "REF", value_parser = parse_metadata_reference)]
        reference: Option<String>,

        /// Filter by exact tag value (e.g. `--tag review`).
        #[arg(long, value_name = "TAG", value_parser = parse_metadata_tag)]
        tag: Option<String>,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,
    },
    /// Detach a tag from a session ref. Outputs the deleted row as JSON.
    #[command(alias = "rm")]
    Remove {
        /// Session ref the tag is attached to.
        #[arg(value_name = "REF", value_parser = parse_metadata_reference)]
        reference: String,

        /// Tag label to remove.
        #[arg(value_name = "TAG", value_parser = parse_metadata_tag)]
        tag: String,
    },
}

pub(super) fn parse_metadata_reference(raw: &str) -> Result<String, String> {
    if raw.len() > REFERENCE_MAX_BYTES {
        return Err(format!(
            "reference must be at most {REFERENCE_MAX_BYTES} bytes"
        ));
    }
    Ok(raw.to_string())
}

fn parse_metadata_tag(raw: &str) -> Result<String, String> {
    if raw.len() > METADATA_TAG_MAX_BYTES {
        return Err(format!(
            "tag must be at most {METADATA_TAG_MAX_BYTES} bytes"
        ));
    }
    Ok(raw.to_string())
}

fn parse_note_id(raw: &str) -> Result<i64, String> {
    let value = raw
        .parse::<i64>()
        .map_err(|e| format!("invalid note id: {e}"))?;
    if value < 1 {
        Err("note id must be at least 1".to_string())
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_metadata_reference_rejects_oversized_values() {
        let raw = "r".repeat(REFERENCE_MAX_BYTES + 1);
        let err = parse_metadata_reference(&raw).unwrap_err();
        assert!(err.contains(&REFERENCE_MAX_BYTES.to_string()));
    }

    #[test]
    fn parse_metadata_tag_rejects_oversized_values() {
        let raw = "t".repeat(METADATA_TAG_MAX_BYTES + 1);
        let err = parse_metadata_tag(&raw).unwrap_err();
        assert!(err.contains(&METADATA_TAG_MAX_BYTES.to_string()));
    }

    #[test]
    fn source_parsers_reuse_config_validation() {
        assert!(parse_source_name("local").is_err());
        assert!(parse_rsync_host("example.test:2222").is_err());
        assert!(parse_rsync_path("/path with spaces").is_err());

        assert_eq!(parse_source_name("laptop").unwrap(), "laptop");
        assert_eq!(
            parse_rsync_host("user@example.test").unwrap(),
            "user@example.test"
        );
        assert_eq!(
            parse_rsync_path("/home/me/.aghist").unwrap(),
            "/home/me/.aghist"
        );
    }
}
