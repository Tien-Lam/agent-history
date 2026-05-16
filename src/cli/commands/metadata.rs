use std::path::PathBuf;

use aghist::config;
use clap::Subcommand;

use super::super::resolvers::parse_transport;

/// Subcommands of `aghist sources` that manage the remote-source registry.
#[derive(Subcommand)]
pub(crate) enum SourcesCommand {
    /// Register a new remote source. Persists to `config.toml`.
    Add {
        /// Stable identifier for the source (used by `remove`).
        #[arg(value_name = "NAME")]
        name: String,
        /// Hostname or `user@host` pointing at the remote machine.
        #[arg(long, value_name = "HOST")]
        host: String,
        /// Path on the remote machine where the agent history lives.
        #[arg(long, value_name = "PATH")]
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
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Pull a remote source's history into a local cache via rsync.
    ///
    /// Mirrors `<host>:<path>/` to `<cache>/sources/<name>/data/` using
    /// rsync. The cache root defaults to the platform cache dir (overridable
    /// via `AGHIST_SOURCES_CACHE_DIR`). After pulling, writes a per-source
    /// manifest with byte/file counts and the pull timestamp; downstream
    /// indexing (federated search, ahist-y3o.6.3) consumes these.
    ///
    /// `--all` pulls every registered source in turn. Pass `--dry-run` to
    /// invoke rsync with `--dry-run` (no files written) - useful to validate
    /// connectivity without mutating the cache. The rsync binary can be
    /// overridden with `AGHIST_RSYNC_BIN` (used by tests; not for end users).
    Pull {
        /// Name of the source to pull. Mutually exclusive with `--all`.
        #[arg(value_name = "NAME", conflicts_with = "all")]
        name: Option<String>,

        /// Pull every registered source.
        #[arg(long, conflicts_with = "name")]
        all: bool,

        /// Run rsync with `--dry-run`; no files are written.
        #[arg(long)]
        dry_run: bool,
    },
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
        #[arg(value_name = "REF")]
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
        #[arg(value_name = "REF")]
        reference: Option<String>,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,
    },
    /// Replace the body of an existing note.
    Edit {
        /// Numeric note id (from `aghist note add` or `aghist note list`).
        #[arg(value_name = "ID")]
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
        #[arg(value_name = "ID")]
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
        #[arg(value_name = "REF")]
        reference: String,

        /// Tag label. Whitespace-trimmed; must be non-empty.
        #[arg(value_name = "TAG")]
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
        #[arg(value_name = "REF")]
        reference: Option<String>,

        /// Filter by exact tag value (e.g. `--tag review`).
        #[arg(long, value_name = "TAG")]
        tag: Option<String>,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,
    },
    /// Detach a tag from a session ref. Outputs the deleted row as JSON.
    #[command(alias = "rm")]
    Remove {
        /// Session ref the tag is attached to.
        #[arg(value_name = "REF")]
        reference: String,

        /// Tag label to remove.
        #[arg(value_name = "TAG")]
        tag: String,
    },
}
