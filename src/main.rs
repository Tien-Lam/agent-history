use aghist::cli_error::{
    ErrorEnvelope, EXIT_EMPTY, EXIT_ERROR, EXIT_OK, EXIT_USAGE,
};
use aghist::model::{CitationRef, ContentBlock, Message, Provider, Role, Session};
use aghist::output::{CommandKind, OutputMode};
use aghist::health::{self, HealthCheck, HealthStatus};
#[cfg(feature = "embeddings")]
use aghist::embed;
use aghist::search::SearchFilters;
use aghist::{app, config, export, mcp, provider, schema, search};

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Parser)]
#[command(name = "aghist", version, about = "Browse and search AI agent conversation history")]
struct Cli {
    /// List sessions without opening the TUI
    #[arg(long)]
    list: bool,

    /// Maximum number of sessions to return when paired with `--list`.
    /// JSON output includes `meta.next_cursor` if more results remain.
    #[arg(long, default_value_t = 20, requires = "list")]
    limit: usize,

    /// Opaque pagination cursor (from a prior `meta.next_cursor`) for `--list`.
    #[arg(long, requires = "list")]
    cursor: Option<String>,

    /// Force rebuild the search index
    #[arg(long)]
    reindex: bool,

    /// Force JSON output (for one-shot commands like --list, export).
    /// Mutually exclusive with --ndjson.
    #[arg(long, global = true)]
    json: bool,

    /// Force newline-delimited JSON output (for streaming commands).
    /// Mutually exclusive with --json.
    #[arg(long, global = true)]
    ndjson: bool,

    #[command(flatten)]
    filters: FilterArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

// Common filter flags shared between `--list` and `search`.
//
// `--has-tool-call` filters at the message level (drops messages without a
// tool invocation); other flags filter at the session or message level
// depending on the subcommand. `--since`/`--until` accept RFC 3339 dates only.
//
// Doc comment intentionally suppressed: clap promotes a flattened struct's
// doc comment to the parent's `about` text, overriding our explicit
// `about = "Browse and search..."` on `Cli`.
#[derive(Debug, Clone, Args)]
struct FilterArgs {
    /// Restrict to a single provider (`claude-code`, `copilot-cli`,
    /// `gemini-cli`, `codex-cli`, `opencode`).
    #[arg(long, global = true, value_parser = parse_provider_slug, value_name = "SLUG")]
    provider: Option<Provider>,

    /// RFC 3339 lower bound on message/session timestamp (inclusive).
    /// Example: `--since 2025-01-01T00:00:00Z`.
    #[arg(long, global = true, value_parser = parse_rfc3339, value_name = "RFC3339")]
    since: Option<DateTime<Utc>>,

    /// RFC 3339 upper bound on message/session timestamp (inclusive).
    #[arg(long, global = true, value_parser = parse_rfc3339, value_name = "RFC3339")]
    until: Option<DateTime<Utc>>,

    /// Substring match against the session's project name (case-insensitive).
    #[arg(long, global = true, value_name = "NAME")]
    project: Option<String>,

    /// Restrict to messages with this role: `user`, `assistant`, or `tool`.
    #[arg(long, global = true, value_parser = parse_role_slug, value_name = "ROLE")]
    role: Option<Role>,

    /// Keep only messages (or sessions containing messages) that include a
    /// tool invocation. Has no effect on session-level lookups that do not
    /// load message content.
    #[arg(long, global = true)]
    has_tool_call: bool,
}

impl FilterArgs {
    fn to_search_filters(&self) -> SearchFilters {
        SearchFilters {
            provider: self.provider,
            since: self.since,
            until: self.until,
            project: self.project.clone(),
            role: self.role,
            has_tool_call: self.has_tool_call,
        }
    }

}

fn parse_role_slug(raw: &str) -> Result<Role, String> {
    Role::from_slug(raw).ok_or_else(|| {
        format!("unknown role '{raw}'. Valid: user, assistant, tool")
    })
}

fn parse_rfc3339(raw: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("invalid RFC 3339 timestamp '{raw}': {e}"))
}

#[derive(Subcommand)]
enum Command {
    /// Export a session to Markdown, JSON, or HTML
    Export {
        /// Output format: md, json, html
        #[arg(long, short)]
        format: export::ExportFormat,

        /// Session ID (or prefix) to export
        #[arg(long, short)]
        session: String,

        /// Output file path (defaults to stdout)
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Slice the session by 1-based turn range (e.g. `12:25`, `:10`, `5:`, or `7`).
        /// Bounds are inclusive. Out-of-range bounds clamp to the available messages.
        #[arg(long)]
        turn_range: Option<String>,
    },
    /// Build or refresh the search index. Idempotent and delta-aware.
    ///
    /// Skips sessions whose source files are unchanged since the last run,
    /// re-indexes those that have changed, and indexes any new sessions.
    /// Always exits with status 0 on success and prints a JSON summary
    /// of `added` / `updated` / `unchanged` counts to stdout.
    Index {
        /// Reindex only sessions from this provider
        /// (`claude-code`, `copilot-cli`, `gemini-cli`, `codex-cli`, `opencode`).
        #[arg(long, value_parser = parse_provider_slug)]
        provider: Option<Provider>,

        /// Force a full rebuild by clearing the index first.
        #[arg(long)]
        force: bool,

        /// Authorise the one-off download of the embedding model
        /// (~90 MB `AllMiniLML6V2`). Required the first time semantic indexing
        /// runs; consent is persisted next to the index, so subsequent runs
        /// don't need this flag. Without consent (and without this flag),
        /// indexing stays purely lexical.
        #[arg(long)]
        accept_download: bool,
    },
    /// Search indexed sessions for a query
    Search {
        /// Tantivy query string (matches content + project fields).
        ///
        /// Omit when reading the query from `--query-file` or `--stdin`.
        #[arg(conflicts_with_all = ["query_file", "stdin"])]
        query: Option<String>,

        /// Read the query from a file (use `-` for stdin).
        ///
        /// Useful for queries containing shell metacharacters (quotes, braces, etc.)
        /// without escaping. Trailing whitespace is stripped.
        #[arg(long, value_name = "PATH", conflicts_with = "stdin")]
        query_file: Option<PathBuf>,

        /// Read the query from standard input (read until EOF).
        ///
        /// Useful for queries containing shell metacharacters (quotes, braces, etc.)
        /// without escaping. Trailing whitespace is stripped.
        #[arg(long)]
        stdin: bool,

        /// Maximum number of hits to return
        #[arg(long, short = 'n', default_value_t = 20)]
        limit: usize,

        /// Opaque pagination cursor from a prior `meta.next_cursor`.
        #[arg(long)]
        cursor: Option<String>,

        /// Force JSON output (default: JSON on pipe, table on TTY)
        #[arg(long)]
        json: bool,

        /// Long-running stream: emit one NDJSON line per new hit as sessions land.
        ///
        /// First poll backfills all existing matches up to `--limit`, then each
        /// subsequent poll emits only previously-unseen `(session_id, message_id)`
        /// hits. Useful for an "agent of agents" watching another agent's progress.
        /// Output is NDJSON regardless of TTY; `--json` is implied.
        #[arg(long)]
        watch: bool,

        /// Poll interval in milliseconds when `--watch` is set (default 2000).
        #[arg(long, default_value_t = 2000, value_name = "MS")]
        watch_interval_ms: u64,

        /// Stop watch mode after N polls (0 = run until interrupted; default 0).
        ///
        /// Mostly useful for tests and one-shot snapshots.
        #[arg(long, default_value_t = 0, value_name = "N")]
        watch_iterations: u32,

        /// Show BM25 score breakdown per result (Tantivy explanation tree).
        /// Useful for tuning relevance and surfacing ranking surprises.
        #[arg(long)]
        debug_search: bool,
    },
    /// Machine-readable doctor: validates index, manifest, and provider state.
    ///
    /// Exits 0 if all checks pass (or only warn), 1 if any check fails. The
    /// JSON envelope is `{ok, checks:[{name, status, hint?}], summary}` so
    /// agents can branch on individual check kinds.
    Health,
    /// List detected provider sources: paths, session counts, sizes, last-indexed-at.
    ///
    /// Helps diagnose "why isn't my session showing up?" — agents (and humans)
    /// can see which provider directories aghist scanned, how many sessions it
    /// found, and when the search index was last updated.
    Sources,
    /// Resolve a citation ref `<provider>/<session-id>#<turn>` to one message.
    Show {
        /// Citation ref. E.g. `claude-code/abc-123#7`.
        #[arg(value_name = "REF")]
        reference: String,

        /// Output format: md (default), json, text.
        #[arg(long, short, default_value = "md")]
        format: ShowFormat,

        /// Include N turns before and after the target for context (default 0).
        #[arg(long, default_value_t = 0)]
        include_context: u32,
    },
    /// Heuristic-extract candidate architectural decisions from sessions.
    ///
    /// Scores each sentence against decision-marker phrases (e.g. "we
    /// decided", "instead of") and returns the top candidates with their
    /// citation refs. v1 is intentionally LLM-free — agents that want
    /// richer extraction can post-process by `aghist show`-ing the refs.
    Decisions {
        /// Restrict to a single session by id, unique id prefix, or full
        /// citation ref `<provider>/<session-id>#<turn>` (turn ignored).
        #[arg(long, short = 's', value_name = "SESSION_OR_REF")]
        session: Option<String>,

        /// Drop sentences whose score is below this threshold.
        /// Default 3.0 keeps explicit decisions and pairs of soft markers.
        #[arg(long, default_value_t = aghist::decisions::DEFAULT_THRESHOLD, value_name = "FLOAT")]
        threshold: f32,

        /// Maximum number of candidates to return across all sessions,
        /// after sorting by score descending.
        #[arg(long, short = 'n', default_value_t = 50)]
        limit: usize,

        /// Force JSON output (default: JSON on pipe, table on TTY).
        #[arg(long)]
        json: bool,
    },
    /// Run a stdio MCP server exposing aghist's read paths to agents.
    ///
    /// Speaks JSON-RPC 2.0 over stdin/stdout with newline-delimited messages,
    /// per the MCP stdio transport. Tools: `search_sessions`, `list_sessions`,
    /// `get_session`, `get_message`, `reindex`, `health`.
    Mcp,
    /// Emit JSON-Schema (draft-2020-12) for an aghist subcommand.
    ///
    /// Lets agents discover params, response shapes, and exit codes without
    /// scraping `--help`. Use `--list` to enumerate available subcommands or
    /// `--all` to dump every schema in one document.
    Schema {
        /// Subcommand name (e.g. `search`, `list`, `health`). Omit with `--list` or `--all`.
        #[arg(value_name = "SUBCMD")]
        subcommand: Option<String>,

        /// List available schema subcommand names as JSON.
        #[arg(long, conflicts_with_all = ["all", "subcommand"])]
        list: bool,

        /// Emit every schema as one object keyed by subcommand name.
        #[arg(long, conflicts_with_all = ["list", "subcommand"])]
        all: bool,
    },
    /// Update aghist to the latest release
    Update,
    /// Remove aghist binary and data
    Uninstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShowFormat {
    Md,
    Json,
    Text,
}

impl std::str::FromStr for ShowFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "md" | "markdown" => Ok(Self::Md),
            "json" => Ok(Self::Json),
            "text" | "txt" => Ok(Self::Text),
            _ => Err(format!("unknown format '{s}' (expected: md, json, text)")),
        }
    }
}

fn parse_provider_slug(raw: &str) -> Result<Provider, String> {
    Provider::from_slug(raw).ok_or_else(|| {
        format!(
            "unknown provider slug '{raw}'. Valid: claude-code, copilot-cli, gemini-cli, codex-cli, opencode"
        )
    })
}

/// Parse a 1-based inclusive turn range against a session of `total` messages.
///
/// Accepts `A:B`, `:B`, `A:`, or a bare `A`. Empty halves default to the
/// session bounds (`1` and `total`). Bounds are clamped to the available
/// range so callers can do `--turn-range :999` without failing.
///
/// Returns `(start, end)` with `1 <= start <= end <= total`, ready to be
/// converted to a 0-based half-open slice via `start-1 .. end`.
fn parse_turn_range(spec: &str, total: usize) -> Result<(usize, usize), String> {
    if total == 0 {
        return Err("session has no messages to slice".to_string());
    }
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(format!("empty turn range '{spec}'"));
    }

    let parse_bound = |s: &str, label: &str| -> Result<Option<usize>, String> {
        if s.is_empty() {
            return Ok(None);
        }
        s.parse::<usize>()
            .map(Some)
            .map_err(|_| format!("invalid {label} '{s}' in turn range '{spec}'"))
    };

    let (start_raw, end_raw) = if let Some((a, b)) = trimmed.split_once(':') {
        (parse_bound(a, "start")?, parse_bound(b, "end")?)
    } else {
        let n = parse_bound(trimmed, "turn")?
            .ok_or_else(|| format!("empty turn range '{spec}'"))?;
        (Some(n), Some(n))
    };

    if start_raw == Some(0) || end_raw == Some(0) {
        return Err(format!("turn range '{spec}' uses 0 (turns are 1-based)"));
    }

    let start = start_raw.unwrap_or(1);
    let end = end_raw.unwrap_or(total).min(total);

    if start > total {
        return Err(format!(
            "turn range '{spec}' starts at {start} but session only has {total} message(s)"
        ));
    }
    if end < start {
        return Err(format!(
            "turn range '{spec}' has end ({end}) before start ({start})"
        ));
    }
    Ok((start, end))
}

#[cfg(test)]
mod turn_range_tests {
    use super::parse_turn_range;

    #[test]
    fn full_range() {
        assert_eq!(parse_turn_range("3:7", 10).unwrap(), (3, 7));
    }

    #[test]
    fn open_start_defaults_to_one() {
        assert_eq!(parse_turn_range(":5", 10).unwrap(), (1, 5));
    }

    #[test]
    fn open_end_defaults_to_total() {
        assert_eq!(parse_turn_range("4:", 10).unwrap(), (4, 10));
    }

    #[test]
    fn single_turn() {
        assert_eq!(parse_turn_range("7", 10).unwrap(), (7, 7));
    }

    #[test]
    fn end_clamps_to_total() {
        assert_eq!(parse_turn_range("3:999", 10).unwrap(), (3, 10));
    }

    #[test]
    fn empty_session_rejects() {
        assert!(parse_turn_range("1:1", 0).is_err());
    }

    #[test]
    fn zero_rejected() {
        assert!(parse_turn_range("0:5", 10).is_err());
        assert!(parse_turn_range("3:0", 10).is_err());
        assert!(parse_turn_range("0", 10).is_err());
    }

    #[test]
    fn start_past_end_rejects() {
        assert!(parse_turn_range("8:3", 10).is_err());
    }

    #[test]
    fn start_past_session_rejects() {
        assert!(parse_turn_range("99:100", 10).is_err());
    }

    #[test]
    fn non_numeric_rejects() {
        assert!(parse_turn_range("a:b", 10).is_err());
        assert!(parse_turn_range("abc", 10).is_err());
    }

    #[test]
    fn empty_spec_rejects() {
        assert!(parse_turn_range("", 10).is_err());
    }
}

fn init_tracing() {
    // Log to ~/.aghist/aghist.log — safe for TUI since it doesn't touch stdout/stderr
    let log_dir = directories::BaseDirs::new()
        .map_or_else(|| PathBuf::from("."), |d| d.home_dir().join(".aghist"));
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "aghist.log");
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("aghist=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(file_appender).with_ansi(false))
        .init();
}

fn main() -> ExitCode {
    init_tracing();
    color_eyre::install().ok();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => return handle_clap_error(&err),
    };

    match run(cli) {
        Ok(code) => exit_code(code),
        Err(env) => {
            env.emit();
            exit_code(EXIT_ERROR)
        }
    }
}

fn handle_clap_error(err: &clap::Error) -> ExitCode {
    // Help / version output is not a failure: let clap print to stdout and
    // return success without an envelope.
    if !err.use_stderr() {
        let _ = err.print();
        return ExitCode::SUCCESS;
    }
    let message = err
        .to_string()
        .lines()
        .find(|line| !line.is_empty())
        .unwrap_or("invalid arguments")
        .trim_start_matches("error: ")
        .to_string();
    ErrorEnvelope::new("usage", message)
        .with_hint("Run `aghist --help` for usage.")
        .emit();
    exit_code(EXIT_USAGE)
}

fn exit_code(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}

#[allow(clippy::too_many_lines)]
fn run(cli: Cli) -> Result<i32, ErrorEnvelope> {
    if cli.reindex {
        let index_dir = search::SearchIndex::default_index_dir();
        if let Ok(index) = search::SearchIndex::open_or_create(&index_dir) {
            let _ = index.clear();
            eprintln!("Search index cleared. Will rebuild on next launch.");
        }
    }

    if cli.json && cli.ndjson {
        ErrorEnvelope::new("usage", "--json and --ndjson are mutually exclusive")
            .with_hint("Pick one. Without either, output auto-detects: JSON/NDJSON on a pipe, human format on a TTY.")
            .emit();
        return Ok(EXIT_USAGE);
    }

    let config = config::Config::load();
    let enabled = config.enabled_providers();
    let providers: Vec<_> = provider::detect_all_providers()
        .into_iter()
        .filter(|p| enabled.contains(&p.provider()))
        .collect();

    match cli.command {
        Some(Command::Mcp) => return run_mcp(providers),
        Some(Command::Schema { subcommand, list, all }) => {
            return schema_command(subcommand.as_deref(), list, all);
        }
        Some(Command::Update) => return self_update(),
        Some(Command::Uninstall) => return uninstall(),
        Some(Command::Export {
            format,
            session,
            output,
            turn_range,
        }) => return export_session(&providers, format, &session, output.as_deref(), turn_range.as_deref()),
        Some(Command::Index {
            provider,
            force,
            accept_download,
        }) => {
            return run_index(&providers, provider, force, accept_download);
        }
        Some(Command::Search {
            query,
            query_file,
            stdin,
            limit,
            cursor,
            json,
            watch,
            watch_interval_ms,
            watch_iterations,
            debug_search,
        }) => {
            let filters = cli.filters.to_search_filters();
            if watch {
                return search_watch_command(
                    &providers,
                    query.as_deref(),
                    query_file.as_deref(),
                    stdin,
                    limit,
                    watch_interval_ms,
                    watch_iterations,
                    &filters,
                );
            }
            return search_command(
                &providers,
                query.as_deref(),
                query_file.as_deref(),
                stdin,
                limit,
                cursor.as_deref(),
                json,
                &filters,
                debug_search,
            );
        }
        Some(Command::Show {
            reference,
            format,
            include_context,
        }) => return show_command(&providers, &reference, format, include_context),
        Some(Command::Decisions {
            session,
            threshold,
            limit,
            json,
        }) => {
            return decisions_command(
                &providers,
                session.as_deref(),
                threshold,
                limit,
                json,
                &cli.filters,
            );
        }
        Some(Command::Sources) => {
            let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::OneShot);
            return sources_command(&providers, mode);
        }
        Some(Command::Health) => {
            let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::OneShot);
            return health_command(&providers, mode);
        }
        None => {}
    }

    if cli.list {
        let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::Streaming);
        return list_sessions(&providers, mode, cli.limit, cli.cursor.as_deref(), &cli.filters);
    }

    run_tui(providers, config)
}

fn run_tui(
    providers: Vec<Box<dyn provider::HistoryProvider>>,
    config: config::Config,
) -> Result<i32, ErrorEnvelope> {
    // Install panic hook that restores the terminal before printing the panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));

    enable_raw_mode().map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to enable raw mode: {e}"))
    })?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to enter alternate screen: {e}"))
    })?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to construct terminal: {e}"))
    })?;

    let mut app = app::App::new(providers, config);
    let result = app.run(&mut terminal);

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
        .map(|()| EXIT_OK)
        .map_err(|e| ErrorEnvelope::new("internal-error", format!("{e:#}")))
}

fn schema_command(
    subcommand: Option<&str>,
    list: bool,
    all: bool,
) -> Result<i32, ErrorEnvelope> {
    let payload = if list {
        schema::subcommand_index()
    } else if all {
        schema::all_schemas()
    } else if let Some(name) = subcommand {
        if let Some(value) = schema::schema_for(name) {
            value
        } else {
            let valid = schema::SUBCOMMANDS.join(", ");
            return Err(ErrorEnvelope::new(
                "usage",
                format!("unknown schema subcommand '{name}'"),
            )
            .with_hint(format!("Valid subcommands: {valid}")));
        }
    } else {
        ErrorEnvelope::new(
            "usage",
            "schema requires <SUBCMD>, --list, or --all",
        )
        .with_hint("Run `aghist schema --list` to see available subcommands.")
        .emit();
        return Ok(EXIT_USAGE);
    };

    serde_json::to_writer(io::stdout().lock(), &payload).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to write schema output: {e}"))
    })?;
    println!();
    Ok(EXIT_OK)
}

fn run_mcp(
    providers: Vec<Box<dyn provider::HistoryProvider>>,
) -> Result<i32, ErrorEnvelope> {
    let stdin = io::stdin().lock();
    let stdout = io::stdout().lock();
    let server = mcp::McpServer::new(providers);
    server.serve(stdin, stdout).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("MCP server stdio error: {e}"))
    })?;
    Ok(EXIT_OK)
}

fn run_index(
    providers: &[Box<dyn provider::HistoryProvider>],
    filter: Option<Provider>,
    force: bool,
    accept_download: bool,
) -> Result<i32, ErrorEnvelope> {
    let started = std::time::Instant::now();

    let active: Vec<&Box<dyn provider::HistoryProvider>> = providers
        .iter()
        .filter(|p| filter.is_none_or(|want| p.provider() == want))
        .collect();

    if let Some(want) = filter {
        if active.is_empty() {
            return Err(ErrorEnvelope::new(
                "provider-unavailable",
                format!(
                    "provider '{}' is not enabled or not detected on this system",
                    want.slug()
                ),
            )
            .with_hint("Enable the provider in your config (`providers` table)."));
        }
    }

    let mut sessions: Vec<Session> = Vec::new();
    let mut errors: Vec<(Provider, String)> = Vec::new();
    for p in &active {
        match p.discover_sessions() {
            Ok(s) => sessions.extend(s),
            Err(e) => errors.push((p.provider(), e.to_string())),
        }
    }

    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new(
            "index-error",
            format!("failed to open index at {}: {e}", index_dir.display()),
        )
    })?;
    if force {
        index.clear().map_err(|e| {
            ErrorEnvelope::new("index-error", format!("failed to clear index: {e}"))
        })?;
    }

    let (tx, _rx) = crossbeam_channel::unbounded();
    // build_index needs the full provider list for load_messages dispatch;
    // provider filtering is enforced by only feeding it sessions from `active`.
    let stats = index.build_index(&sessions, providers, &tx).map_err(|e| {
        ErrorEnvelope::new("index-error", format!("failed to build index: {e}"))
    })?;

    let embed_summary = run_embeddings(&index_dir, &sessions, providers, accept_download)?;

    let provider_slugs: Vec<&'static str> = active.iter().map(|p| p.provider().slug()).collect();
    let summary = serde_json::json!({
        "providers": provider_slugs,
        "sessions_total": sessions.len(),
        "added": stats.added,
        "updated": stats.updated,
        "unchanged": stats.unchanged,
        "messages_indexed": stats.messages_indexed,
        "force": force,
        "index_dir": index_dir.display().to_string(),
        "duration_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "errors": errors
            .iter()
            .map(|(p, msg)| serde_json::json!({ "provider": p.slug(), "error": msg }))
            .collect::<Vec<_>>(),
        "embeddings": embed_summary,
    });

    println!("{summary}");
    Ok(EXIT_OK)
}

/// Drive the (opt-in) semantic side of indexing.
///
/// Three states feed the JSON summary back to the caller:
///
/// - `disabled`: the binary was built without the `embeddings` feature, so we
///   surface that even when `--accept-download` is passed (users would
///   otherwise see silent no-ops).
/// - `awaiting-consent`: feature is compiled in, no consent file exists, and
///   `--accept-download` was not passed. Lexical indexing still happened.
/// - `enabled`: consent recorded (just now or in a prior run); embeddings
///   were generated and persisted.
// The `embeddings`-disabled variant can't fail, but the `embeddings`-enabled
// variant can — both signatures need to match so callers don't change shape.
#[cfg(not(feature = "embeddings"))]
#[allow(clippy::unnecessary_wraps)]
fn run_embeddings(
    _index_dir: &std::path::Path,
    _sessions: &[Session],
    _providers: &[Box<dyn provider::HistoryProvider>],
    accept_download: bool,
) -> Result<serde_json::Value, ErrorEnvelope> {
    Ok(serde_json::json!({
        "status": "disabled",
        "reason": "binary built without `embeddings` feature",
        "accept_download_requested": accept_download,
    }))
}

#[cfg(feature = "embeddings")]
fn run_embeddings(
    index_dir: &std::path::Path,
    sessions: &[Session],
    providers: &[Box<dyn provider::HistoryProvider>],
    accept_download: bool,
) -> Result<serde_json::Value, ErrorEnvelope> {
    let consent = embed::Consent::load(index_dir);
    let consent = match (consent, accept_download) {
        (Some(c), _) => c,
        (None, true) => embed::Consent::record(index_dir, embed::DEFAULT_MODEL).map_err(|e| {
            ErrorEnvelope::new(
                "embed-error",
                format!("failed to record embedding-download consent: {e}"),
            )
        })?,
        (None, false) => {
            return Ok(serde_json::json!({
                "status": "awaiting-consent",
                "model": embed::DEFAULT_MODEL,
                "hint": "re-run with `--accept-download` to enable semantic indexing",
            }));
        }
    };

    let cache_dir = index_dir.join("models");
    let mut embedder = embed::Embedder::try_new(&cache_dir).map_err(|e| {
        ErrorEnvelope::new(
            "embed-error",
            format!("failed to initialise embedder: {e}"),
        )
    })?;

    // On a schema bump (STORE_VERSION mismatch), evict the old sidecar and
    // start fresh — the alternative would be to refuse to reindex, which is
    // worse UX than transparently rebuilding. We surface the eviction so it's
    // visible in the JSON summary.
    let mut evicted_old_schema = false;
    let mut store = match embed::EmbeddingStore::open(index_dir) {
        Ok(Some(s)) => s,
        Ok(None) => embed::EmbeddingStore::create(index_dir, embedder.model_slug(), embedder.dim()),
        Err(embed::EmbedError::SchemaMismatch { .. }) => {
            embed::EmbeddingStore::evict(index_dir).map_err(|e| {
                ErrorEnvelope::new(
                    "embed-error",
                    format!("failed to evict outdated embedding store: {e}"),
                )
            })?;
            evicted_old_schema = true;
            embed::EmbeddingStore::create(index_dir, embedder.model_slug(), embedder.dim())
        }
        Err(e) => {
            return Err(ErrorEnvelope::new(
                "embed-error",
                format!("failed to open embedding store: {e}"),
            ));
        }
    };

    let mut errors: Vec<String> = Vec::new();
    let mut messages_embedded = 0usize;
    let mut messages_reused = 0usize;

    for session in sessions {
        let Some(provider) = providers.iter().find(|p| p.provider() == session.provider) else {
            continue;
        };
        let messages = match provider.load_messages(session) {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!("{}: {e}", session.id.0));
                continue;
            }
        };

        // (id, text, content_hash) for messages whose cached vector is stale
        // or absent. We compute the hash up front so the freshness check is a
        // cheap byte compare against what's in the store.
        let pending: Vec<(String, String, [u8; embed::HASH_LEN])> = messages
            .iter()
            .filter_map(|m| {
                let text = collect_text(m);
                if text.trim().is_empty() {
                    return None;
                }
                let hash = embed::content_hash(&text);
                if store.get_if_fresh(&m.id.0, &hash).is_some() {
                    messages_reused += 1;
                    return None;
                }
                Some((m.id.0.clone(), text, hash))
            })
            .collect();

        if pending.is_empty() {
            continue;
        }

        let texts: Vec<String> = pending.iter().map(|(_, t, _)| t.clone()).collect();
        match embedder.embed_batch(&texts) {
            Ok(vectors) => {
                for ((id, _, hash), vec) in pending.into_iter().zip(vectors) {
                    if let Err(e) = store.upsert(&id, hash, vec) {
                        errors.push(format!("{id}: {e}"));
                    } else {
                        messages_embedded += 1;
                    }
                }
            }
            Err(e) => errors.push(format!("{}: {e}", session.id.0)),
        }
    }

    store.flush().map_err(|e| {
        ErrorEnvelope::new(
            "embed-error",
            format!("failed to persist embeddings: {e}"),
        )
    })?;

    Ok(serde_json::json!({
        "status": "enabled",
        "model": consent.model,
        "dim": store.dim(),
        "messages_embedded": messages_embedded,
        "messages_reused_from_cache": messages_reused,
        "messages_total_in_store": store.len(),
        "evicted_old_schema": evicted_old_schema,
        "consent_accepted_at": consent.accepted_at,
        "errors": errors,
    }))
}

#[cfg(feature = "embeddings")]
fn collect_text(message: &Message) -> String {
    use aghist::model::ContentBlock;
    let parts: Vec<&str> = message
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text(t) | ContentBlock::Thinking(t) | ContentBlock::Error(t) => {
                t.as_str()
            }
            ContentBlock::CodeBlock { code, .. } => code.as_str(),
            ContentBlock::ToolUse(tc) => tc.arguments.as_str(),
            ContentBlock::ToolResult(tr) => tr.output.as_str(),
        })
        .collect();
    parts.join("\n")
}

fn export_session(
    providers: &[Box<dyn provider::HistoryProvider>],
    format: export::ExportFormat,
    session_id: &str,
    output: Option<&std::path::Path>,
    turn_range: Option<&str>,
) -> Result<i32, ErrorEnvelope> {
    let mut all_sessions = Vec::new();
    for p in providers {
        if let Ok(sessions) = p.discover_sessions() {
            all_sessions.extend(sessions);
        }
    }

    let session = all_sessions
        .iter()
        .find(|s| s.id.0 == session_id || s.id.0.starts_with(session_id))
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "session-not-found",
                format!("Session not found: {session_id}"),
            )
            .with_hint("Run `aghist --list` to see available session IDs.")
        })?;

    let provider = providers
        .iter()
        .find(|p| p.provider() == session.provider)
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "provider-unavailable",
                format!(
                    "Provider {} is not enabled for session {}",
                    session.provider, session.id.0
                ),
            )
            .with_hint("Enable the provider in your config (`providers` table).")
        })?;

    let messages = provider.load_messages(session).map_err(|e| {
        ErrorEnvelope::new(
            "provider-error",
            format!("failed to load messages for {}: {e}", session.id.0),
        )
    })?;

    let sliced = match turn_range {
        Some(spec) => {
            let total = messages.len();
            let (start, end) = parse_turn_range(spec, total).map_err(|msg| {
                ErrorEnvelope::new("usage", msg)
                    .with_hint("Use a 1-based range like `12:25`, `:10`, `5:`, or a single turn `7`.")
            })?;
            // start..end are 1-based inclusive bounds; convert to 0-based half-open.
            &messages[(start - 1)..end]
        }
        None => &messages[..],
    };

    let content = export::export(format, session, sliced);

    if let Some(path) = output {
        std::fs::write(path, &content).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to write {}: {e}", path.display()),
            )
        })?;
        eprintln!("Exported to {}", path.display());
    } else {
        print!("{content}");
    }

    Ok(EXIT_OK)
}

fn uninstall() -> Result<i32, ErrorEnvelope> {
    let exe = std::env::current_exe()
        .map_err(|e| ErrorEnvelope::new("io-error", format!("current_exe failed: {e}")))?;
    let index_dir = search::SearchIndex::default_index_dir();
    let config_path = config::Config::config_path();
    let config_dir = config_path.as_deref().and_then(|p| p.parent());

    eprintln!("This will remove:");
    eprintln!("  binary:       {}", exe.display());
    if index_dir.exists() {
        eprintln!("  search index: {}", index_dir.display());
    }
    if let Some(dir) = config_dir {
        if dir.exists() {
            eprintln!("  config:       {}", dir.display());
        }
    }

    eprint!("\nContinue? [y/N] ");
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to read confirmation: {e}")))?;
    if !input.trim().eq_ignore_ascii_case("y") {
        eprintln!("Aborted.");
        return Err(ErrorEnvelope::new("aborted", "uninstall cancelled by user"));
    }

    if index_dir.exists() {
        std::fs::remove_dir_all(&index_dir).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to remove {}: {e}", index_dir.display()),
            )
        })?;
        eprintln!("Removed {}", index_dir.display());
    }
    if let Some(dir) = config_dir {
        if dir.exists() {
            std::fs::remove_dir_all(dir).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to remove {}: {e}", dir.display()),
                )
            })?;
            eprintln!("Removed {}", dir.display());
        }
    }

    // On Windows, self-delete requires renaming first
    #[cfg(windows)]
    {
        let tmp = exe.with_extension("old");
        std::fs::rename(&exe, &tmp).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to rename {} -> {}: {e}", exe.display(), tmp.display()),
            )
        })?;
        if let Err(e) = std::process::Command::new("cmd")
            .args(["/C", "timeout", "/t", "2", "/nobreak", ">nul", "&", "del"])
            .arg(&tmp)
            .spawn()
        {
            eprintln!(
                "warning: could not schedule cleanup of {}: {e}",
                tmp.display()
            );
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(&exe).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to remove {}: {e}", exe.display()),
            )
        })?;
    }

    eprintln!("aghist has been uninstalled.");
    Ok(EXIT_OK)
}

fn self_update() -> Result<i32, ErrorEnvelope> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner("Tien-Lam")
        .repo_name("agent-history")
        .bin_name("aghist")
        .show_download_progress(true)
        .no_confirm(true)
        .current_version(self_update::cargo_crate_version!())
        .build()
        .map_err(|e| {
            ErrorEnvelope::new("update-failed", format!("failed to configure updater: {e}"))
        })?
        .update()
        .map_err(|e| ErrorEnvelope::new("update-failed", format!("update failed: {e}")))?;

    if status.updated() {
        println!("Updated to v{}", status.version());
    } else {
        println!("Already up to date (v{})", status.version());
    }
    Ok(EXIT_OK)
}

fn resolve_search_query(
    query: Option<&str>,
    query_file: Option<&std::path::Path>,
    stdin: bool,
) -> Result<String, ErrorEnvelope> {
    use std::io::Read;

    let mut sources = 0;
    if query.is_some() {
        sources += 1;
    }
    if query_file.is_some() {
        sources += 1;
    }
    if stdin {
        sources += 1;
    }
    if sources == 0 {
        return Err(ErrorEnvelope::new(
            "usage",
            "search requires a query (positional, --query-file, or --stdin)",
        )
        .with_hint("Run `aghist search --help` for usage."));
    }

    if let Some(q) = query {
        return Ok(q.to_string());
    }

    let mut buf = String::new();
    if stdin {
        io::stdin().read_to_string(&mut buf).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to read query from stdin: {e}"))
        })?;
    } else if let Some(path) = query_file {
        if path == std::path::Path::new("-") {
            io::stdin().read_to_string(&mut buf).map_err(|e| {
                ErrorEnvelope::new("io-error", format!("failed to read query from stdin: {e}"))
            })?;
        } else {
            buf = std::fs::read_to_string(path).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to read query file {}: {e}", path.display()),
                )
            })?;
        }
    }

    Ok(buf.trim_end().to_string())
}

/// Tantivy's `TopDocs::with_limit(N)` materializes only N results, so to
/// paginate by keyset we ask for a generous upper bound, sort with the same
/// tie-break the single-page path uses, then slice past the cursor.
const SEARCH_PAGINATION_POOL: usize = 1000;

#[allow(clippy::too_many_arguments)]
fn search_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    query: Option<&str>,
    query_file: Option<&std::path::Path>,
    stdin: bool,
    limit: usize,
    cursor: Option<&str>,
    force_json: bool,
    filters: &SearchFilters,
    debug_search: bool,
) -> Result<i32, ErrorEnvelope> {
    use aghist::model::Session;

    let resolved = match resolve_search_query(query, query_file, stdin) {
        Ok(q) => q,
        Err(env) => {
            env.emit();
            return Ok(EXIT_USAGE);
        }
    };
    let query = resolved.as_str();

    if query.trim().is_empty() {
        ErrorEnvelope::new("usage", "search query is empty")
            .with_hint("Run `aghist search --help` for usage.")
            .emit();
        return Ok(EXIT_USAGE);
    }

    let after = if let Some(token) = cursor {
        if let Ok(c) = aghist::cursor::SearchCursor::decode(token) {
            Some(c)
        } else {
            ErrorEnvelope::new("usage", "invalid --cursor token")
                .with_hint("Cursors are opaque; pass back the `meta.next_cursor` value verbatim.")
                .emit();
            return Ok(EXIT_USAGE);
        }
    } else {
        None
    };

    let mut sessions: Vec<Session> = Vec::new();
    for p in providers {
        if let Ok(found) = p.discover_sessions() {
            sessions.extend(found);
        }
    }

    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new("index-error", format!("failed to open search index: {e}"))
    })?;

    // Incremental index update — fast on subsequent calls (manifest tracks mtimes).
    // We don't surface progress for the CLI, so drain into a sender we discard.
    let (tx, _rx) = crossbeam_channel::unbounded::<aghist::action::Action>();
    index.build_index(&sessions, providers, &tx).map_err(|e| {
        ErrorEnvelope::new("index-error", format!("failed to build search index: {e}"))
    })?;

    // Always pull the pagination pool so cursor resumption sees a stable
    // ordering across calls. Tantivy ranks by score, but we re-sort below
    // with our deterministic tie-break.
    let pool_size = if cursor.is_some() {
        SEARCH_PAGINATION_POOL
    } else {
        limit.max(1)
    };
    let raw_hits: Vec<(search::SearchHit, Option<search::Explanation>)> = if debug_search {
        index
            .search_with_filters_and_explain(query, pool_size, filters)
            .map_err(|e| ErrorEnvelope::new("index-error", format!("search failed: {e}")))?
            .into_iter()
            .map(|(h, e)| (h, Some(e)))
            .collect()
    } else {
        index
            .search_with_filters(query, pool_size, filters)
            .map_err(|e| ErrorEnvelope::new("index-error", format!("search failed: {e}")))?
            .into_iter()
            .map(|h| (h, None))
            .collect()
    };

    let total = raw_hits.len();

    if raw_hits.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    // Tie-break by (started_at DESC, session_id ASC) for deterministic ordering.
    // Tantivy already returns score-DESC; we use a stable sort to preserve that
    // and only reorder ties.
    let session_meta: std::collections::HashMap<&str, &Session> =
        sessions.iter().map(|s| (s.id.0.as_str(), s)).collect();

    let mut ordered = raw_hits;
    ordered.sort_by(|a, b| {
        b.0.score
            .partial_cmp(&a.0.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_started = session_meta
                    .get(a.0.session_id.as_str())
                    .map(|s| s.started_at);
                let b_started = session_meta
                    .get(b.0.session_id.as_str())
                    .map(|s| s.started_at);
                b_started.cmp(&a_started)
            })
            .then_with(|| a.0.session_id.cmp(&b.0.session_id))
    });

    let page_start = match &after {
        Some(c) => ordered
            .iter()
            .position(|(h, _)| {
                // Strictly past the cursor in score-DESC, id-ASC order.
                // Exact f32 equality is intentional — both sides come from
                // Tantivy's deterministic scoring for the same query, not
                // arithmetic that would introduce floating-point drift.
                #[allow(clippy::float_cmp)]
                {
                    h.score < c.score || (h.score == c.score && h.session_id > c.session_id)
                }
            })
            .unwrap_or(ordered.len()),
        None => 0,
    };

    let page_end = page_start.saturating_add(limit).min(ordered.len());
    let page = &ordered[page_start..page_end];

    if page.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let next_cursor = if page_end < ordered.len() {
        page.last().map(|(h, _)| {
            aghist::cursor::SearchCursor {
                score: h.score,
                session_id: h.session_id.clone(),
            }
            .encode()
        })
    } else {
        None
    };

    let want_json = force_json || !io::stdout().is_terminal();

    if want_json {
        print_search_json(page, &session_meta, total, next_cursor.as_deref()).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?;
    } else {
        print_search_table(page, &session_meta, next_cursor.as_deref());
    }

    Ok(EXIT_OK)
}

fn print_search_json(
    hits: &[(search::SearchHit, Option<search::Explanation>)],
    sessions: &std::collections::HashMap<&str, &aghist::model::Session>,
    total: usize,
    next_cursor: Option<&str>,
) -> std::io::Result<()> {
    #[derive(serde::Serialize)]
    struct JsonHit<'a> {
        session_id: &'a str,
        message_id: &'a str,
        score: f32,
        snippet: &'a str,
        provider: Option<aghist::model::Provider>,
        project: Option<&'a str>,
        started_at: Option<chrono::DateTime<chrono::Utc>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        explanation: Option<&'a search::Explanation>,
    }

    let rows: Vec<JsonHit> = hits
        .iter()
        .map(|(h, explain)| {
            let session = sessions.get(h.session_id.as_str()).copied();
            JsonHit {
                session_id: &h.session_id,
                message_id: &h.message_id,
                score: h.score,
                snippet: &h.snippet,
                provider: session.map(|s| s.provider),
                project: session.and_then(|s| s.project_name.as_deref()),
                started_at: session.map(|s| s.started_at),
                explanation: explain.as_ref(),
            }
        })
        .collect();

    let doc = serde_json::json!({
        "hits": rows,
        "meta": { "next_cursor": next_cursor, "total": total },
    });
    serde_json::to_writer(io::stdout().lock(), &doc)?;
    println!();
    Ok(())
}

fn print_search_table(
    hits: &[(search::SearchHit, Option<search::Explanation>)],
    sessions: &std::collections::HashMap<&str, &aghist::model::Session>,
    next_cursor: Option<&str>,
) {
    println!(
        "{:<6}  {:<16}  {:<12}  {:<20}  {:<14}  SNIPPET",
        "SCORE", "STARTED", "PROVIDER", "PROJECT", "SESSION"
    );
    for (h, explain) in hits {
        let session = sessions.get(h.session_id.as_str()).copied();
        let started = session
            .map(|s| s.started_at.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        let provider = session.map_or("", |s| s.provider.as_str());
        let project = session
            .and_then(|s| s.project_name.as_deref())
            .unwrap_or("");
        let project = truncate(project, 20);
        let session_short = truncate(&h.session_id, 14);
        let snippet = truncate(&h.snippet, 80);
        println!(
            "{:<6.2}  {:<16}  {:<12}  {:<20}  {:<14}  {}",
            h.score, started, provider, project, session_short, snippet
        );
        if let Some(explanation) = explain {
            for line in explanation.to_pretty_json().lines() {
                println!("    {line}");
            }
        }
    }
    if let Some(token) = next_cursor {
        println!("\n(more results — pass --cursor {token} for the next page)");
    }
}

/// Long-running NDJSON stream: poll for newly-indexed sessions and emit
/// previously-unseen hits matching `query`.
///
/// First iteration backfills all current matches (so a fresh subscriber sees
/// existing state); subsequent iterations only emit `(session_id, message_id)`
/// pairs that have not been emitted before. Exits cleanly on broken pipe so
/// `aghist search ... --watch | head -N` works.
#[allow(clippy::too_many_arguments)]
fn search_watch_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    query: Option<&str>,
    query_file: Option<&std::path::Path>,
    stdin: bool,
    limit: usize,
    interval_ms: u64,
    max_iterations: u32,
    filters: &SearchFilters,
) -> Result<i32, ErrorEnvelope> {
    use aghist::model::Session;
    use std::collections::HashSet;
    use std::io::Write;

    let resolved = match resolve_search_query(query, query_file, stdin) {
        Ok(q) => q,
        Err(env) => {
            env.emit();
            return Ok(EXIT_USAGE);
        }
    };
    let query = resolved.as_str();
    if query.trim().is_empty() {
        ErrorEnvelope::new("usage", "search query is empty")
            .with_hint("Run `aghist search --help` for usage.")
            .emit();
        return Ok(EXIT_USAGE);
    }

    let index_dir = search::SearchIndex::default_index_dir();
    let index = search::SearchIndex::open_or_create(&index_dir).map_err(|e| {
        ErrorEnvelope::new("index-error", format!("failed to open search index: {e}"))
    })?;

    let interval = std::time::Duration::from_millis(interval_ms);
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut iteration: u32 = 0;
    let stdout = io::stdout();

    loop {
        iteration += 1;

        let mut sessions: Vec<Session> = Vec::new();
        for p in providers {
            if let Ok(found) = p.discover_sessions() {
                sessions.extend(found);
            }
        }

        let (tx, _rx) = crossbeam_channel::unbounded::<aghist::action::Action>();
        index.build_index(&sessions, providers, &tx).map_err(|e| {
            ErrorEnvelope::new("index-error", format!("failed to build search index: {e}"))
        })?;

        let hits = index
            .search_with_filters(query, limit, filters)
            .map_err(|e| ErrorEnvelope::new("index-error", format!("search failed: {e}")))?;

        let session_meta: std::collections::HashMap<&str, &Session> =
            sessions.iter().map(|s| (s.id.0.as_str(), s)).collect();

        let mut handle = stdout.lock();
        for h in &hits {
            let key = (h.session_id.clone(), h.message_id.clone());
            if !seen.insert(key) {
                continue;
            }
            if write_watch_hit(&mut handle, h, &session_meta).is_err() {
                // Broken pipe (downstream closed) — exit cleanly.
                return Ok(EXIT_OK);
            }
        }
        // Flush so consumers see lines promptly between sleeps.
        if handle.flush().is_err() {
            return Ok(EXIT_OK);
        }
        drop(handle);

        if max_iterations > 0 && iteration >= max_iterations {
            return Ok(EXIT_OK);
        }

        std::thread::sleep(interval);
    }
}

fn write_watch_hit<W: std::io::Write>(
    out: &mut W,
    hit: &search::SearchHit,
    sessions: &std::collections::HashMap<&str, &aghist::model::Session>,
) -> std::io::Result<()> {
    #[derive(serde::Serialize)]
    struct JsonHit<'a> {
        session_id: &'a str,
        message_id: &'a str,
        score: f32,
        snippet: &'a str,
        provider: Option<aghist::model::Provider>,
        project: Option<&'a str>,
        started_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    let session = sessions.get(hit.session_id.as_str()).copied();
    let row = JsonHit {
        session_id: &hit.session_id,
        message_id: &hit.message_id,
        score: hit.score,
        snippet: &hit.snippet,
        provider: session.map(|s| s.provider),
        project: session.and_then(|s| s.project_name.as_deref()),
        started_at: session.map(|s| s.started_at),
    };
    serde_json::to_writer(&mut *out, &row)?;
    out.write_all(b"\n")?;
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn list_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    mode: OutputMode,
    limit: usize,
    cursor: Option<&str>,
    filters: &FilterArgs,
) -> Result<i32, ErrorEnvelope> {
    let mut all_sessions = Vec::new();

    let needs_messages = filters.role.is_some() || filters.has_tool_call;
    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    for p in providers {
        // When --provider is set, skip non-matching providers entirely so we
        // don't pay discovery cost for sessions we'd just throw away.
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        match p.discover_sessions() {
            Ok(sessions) => {
                let kept: Vec<Session> = sessions
                    .into_iter()
                    .filter(|s| session_matches(s, filters, project_needle.as_deref()))
                    .filter(|s| {
                        !needs_messages || session_has_matching_message(p.as_ref(), s, filters)
                    })
                    .collect();
                if !mode.is_machine() {
                    println!("{}: {} sessions", p.provider(), kept.len());
                }
                all_sessions.extend(kept);
            }
            Err(e) => {
                eprintln!("{}: error: {e}", p.provider());
            }
        }
    }

    // Canonical sort: started_at DESC, session_id ASC. The id tie-break makes
    // the cursor's keyset comparison total even when two sessions share a
    // millisecond timestamp.
    all_sessions.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| a.id.0.cmp(&b.id.0))
    });

    let total = all_sessions.len();

    let after = if let Some(token) = cursor {
        if let Ok(c) = aghist::cursor::ListCursor::decode(token) {
            Some(c)
        } else {
            ErrorEnvelope::new("usage", "invalid --cursor token")
                .with_hint("Cursors are opaque; pass back the `meta.next_cursor` value verbatim.")
                .emit();
            return Ok(EXIT_USAGE);
        }
    } else {
        None
    };

    let page_start = match &after {
        Some(c) => all_sessions
            .iter()
            .position(|s| {
                // Match the canonical order: started_at DESC, id ASC. We want
                // the first session strictly *after* the cursor key.
                s.started_at < c.started_at
                    || (s.started_at == c.started_at && s.id.0 > c.session_id)
            })
            .unwrap_or(all_sessions.len()),
        None => 0,
    };

    let page_end = page_start.saturating_add(limit).min(all_sessions.len());
    let page = &all_sessions[page_start..page_end];

    let next_cursor = if page_end < all_sessions.len() {
        page.last().map(|s| {
            aghist::cursor::ListCursor {
                started_at: s.started_at,
                session_id: s.id.0.clone(),
            }
            .encode()
        })
    } else {
        None
    };

    match mode {
        OutputMode::Human => render_list_human(page, total, next_cursor.as_deref()),
        OutputMode::Json => render_list_json(page, total, next_cursor.as_deref()).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?,
        OutputMode::Ndjson => {
            render_list_ndjson(page, total, next_cursor.as_deref()).map_err(|e| {
                ErrorEnvelope::new("io-error", format!("failed to write NDJSON output: {e}"))
            })?;
        }
    }

    if all_sessions.is_empty() {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

/// Apply session-level filters (provider, since/until, project). Provider is
/// not re-checked here when the caller already filtered by provider, but it's
/// harmless to do so. `project_needle` is the pre-lowercased substring for
/// efficiency in the per-session loop.
fn session_matches(
    session: &Session,
    filters: &FilterArgs,
    project_needle: Option<&str>,
) -> bool {
    if let Some(want) = filters.provider {
        if session.provider != want {
            return false;
        }
    }
    if let Some(since) = filters.since {
        if session.started_at < since {
            return false;
        }
    }
    if let Some(until) = filters.until {
        if session.started_at > until {
            return false;
        }
    }
    if let Some(needle) = project_needle {
        let project = session
            .project_name
            .as_deref()
            .map(str::to_lowercase)
            .unwrap_or_default();
        if !project.contains(needle) {
            return false;
        }
    }
    true
}

/// Returns true if the session contains at least one message satisfying the
/// message-level filters (`--role`, `--has-tool-call`). Loads messages on
/// demand; corrupt/unreadable sessions are silently dropped (consistent with
/// the rest of the pipeline).
fn session_has_matching_message(
    provider: &dyn provider::HistoryProvider,
    session: &Session,
    filters: &FilterArgs,
) -> bool {
    let Ok(messages) = provider.load_messages(session) else {
        return false;
    };
    messages.iter().any(|m| message_matches(m, filters))
}

fn message_matches(message: &Message, filters: &FilterArgs) -> bool {
    if let Some(role) = filters.role {
        if message.role != role {
            return false;
        }
    }
    if filters.has_tool_call
        && !message
            .content
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse(_)))
    {
        return false;
    }
    true
}

fn render_list_human(sessions: &[Session], total: usize, next_cursor: Option<&str>) {
    println!("\nTotal: {total} sessions\n");
    for s in sessions {
        let project = s.project_name.as_deref().unwrap_or("(unknown)");
        let branch = s.git_branch.as_deref().unwrap_or("");
        let summary = match s.summary.as_deref() {
            Some(text) if text.chars().count() > 60 => {
                let mut s: String = text.chars().take(57).collect();
                s.push_str("...");
                s
            }
            Some(text) => text.to_string(),
            None => String::new(),
        };
        println!(
            "  {} | {} | {} | {} | {}",
            s.started_at.format("%Y-%m-%d %H:%M"),
            s.provider,
            project,
            branch,
            summary
        );
    }
    if let Some(token) = next_cursor {
        println!("\n(more results — pass --cursor {token} for the next page)");
    }
}

#[derive(serde::Serialize)]
struct SessionRow<'a> {
    id: &'a str,
    provider: aghist::model::Provider,
    project: Option<&'a str>,
    branch: Option<&'a str>,
    summary: Option<&'a str>,
    started_at: chrono::DateTime<chrono::Utc>,
    message_count: usize,
}

impl<'a> SessionRow<'a> {
    fn from_session(s: &'a Session) -> Self {
        Self {
            id: s.id.0.as_str(),
            provider: s.provider,
            project: s.project_name.as_deref(),
            branch: s.git_branch.as_deref(),
            summary: s.summary.as_deref(),
            started_at: s.started_at,
            message_count: s.message_count,
        }
    }
}

fn render_list_json(
    sessions: &[Session],
    total: usize,
    next_cursor: Option<&str>,
) -> std::io::Result<()> {
    use std::io::Write as _;
    let rows: Vec<SessionRow<'_>> = sessions.iter().map(SessionRow::from_session).collect();
    let doc = serde_json::json!({
        "sessions": rows,
        "meta": { "next_cursor": next_cursor, "total": total },
    });
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &doc).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_list_ndjson(
    sessions: &[Session],
    total: usize,
    next_cursor: Option<&str>,
) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    for s in sessions {
        let row = SessionRow::from_session(s);
        serde_json::to_writer(&mut out, &row).map_err(std::io::Error::other)?;
        writeln!(out)?;
    }
    // Trailing meta record terminates the stream so consumers can detect EOF
    // without watching stdin close. Keyed by `meta` so it never collides with
    // a session row (which is keyed by `id`).
    let meta = serde_json::json!({
        "meta": { "next_cursor": next_cursor, "total": total },
    });
    serde_json::to_writer(&mut out, &meta).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn health_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    let checks = health::run_health_checks(providers);
    let any_failed = checks.iter().any(|c| c.status == HealthStatus::Fail);

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Human => render_health_human(&mut out, &checks),
        OutputMode::Json | OutputMode::Ndjson => render_health_json(&mut out, &checks, !any_failed),
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write health output: {e}")))?;

    Ok(if any_failed { EXIT_ERROR } else { EXIT_OK })
}

fn render_health_human<W: io::Write>(out: &mut W, checks: &[HealthCheck]) -> io::Result<()> {
    let any_failed = checks.iter().any(|c| c.status == HealthStatus::Fail);
    let any_warn = checks.iter().any(|c| c.status == HealthStatus::Warn);
    let summary = if any_failed {
        "FAIL"
    } else if any_warn {
        "WARN"
    } else {
        "OK"
    };
    writeln!(out, "Overall: {summary}")?;
    writeln!(out)?;
    for c in checks {
        let tag = match c.status {
            HealthStatus::Ok => "OK  ",
            HealthStatus::Warn => "WARN",
            HealthStatus::Fail => "FAIL",
        };
        writeln!(out, "  [{tag}] {} — {}", c.name, c.message)?;
        if let Some(hint) = &c.hint {
            writeln!(out, "         hint: {hint}")?;
        }
    }
    Ok(())
}

fn render_health_json<W: io::Write>(
    out: &mut W,
    checks: &[HealthCheck],
    ok: bool,
) -> io::Result<()> {
    let summary = serde_json::json!({
        "ok_count": checks.iter().filter(|c| c.status == HealthStatus::Ok).count(),
        "warn_count": checks.iter().filter(|c| c.status == HealthStatus::Warn).count(),
        "fail_count": checks.iter().filter(|c| c.status == HealthStatus::Fail).count(),
    });
    let payload = serde_json::json!({
        "ok": ok,
        "checks": checks,
        "summary": summary,
    });
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn sources_command(
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
        .map(|p| collect_source_row(p.as_ref(), &index_dir, last_indexed_at))
        .collect();

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Human => render_sources_human(&mut out, &rows, &index_dir, last_indexed_at),
        OutputMode::Json => render_sources_json(&mut out, &rows, &index_dir, last_indexed_at),
        OutputMode::Ndjson => render_sources_ndjson(&mut out, &rows),
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write sources output: {e}")))?;

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

fn collect_source_row(
    p: &dyn provider::HistoryProvider,
    _index_dir: &std::path::Path,
    _last_indexed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> SourceRow {
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
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_file() {
            total = total.saturating_add(meta.len());
        } else if meta.is_dir() {
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
        writeln!(out, "No providers detected. Check your config (`providers` table).")?;
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
    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_sources_ndjson<W: io::Write>(out: &mut W, rows: &[SourceRow]) -> io::Result<()> {
    for row in rows {
        serde_json::to_writer(&mut *out, row).map_err(std::io::Error::other)?;
        writeln!(out)?;
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

fn show_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    raw_ref: &str,
    format: ShowFormat,
    include_context: u32,
) -> Result<i32, ErrorEnvelope> {
    let citation: CitationRef = raw_ref.parse().map_err(|e: aghist::model::CitationParseError| {
        ErrorEnvelope::new("usage", format!("invalid ref '{raw_ref}': {e}"))
            .with_hint("Format: <provider-slug>/<session-id>#<turn>. Example: claude-code/abc-123#7")
    })?;

    let provider = providers
        .iter()
        .find(|p| p.provider() == citation.provider)
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "provider-unavailable",
                format!("provider '{}' is not enabled or not detected", citation.provider.slug()),
            )
            .with_hint("Enable it in your config (`providers` table) or check that the source dir exists.")
        })?;

    let sessions = provider.discover_sessions().map_err(|e| {
        ErrorEnvelope::new(
            "provider-error",
            format!("failed to discover sessions for {}: {e}", citation.provider.slug()),
        )
    })?;

    let session = sessions
        .iter()
        .find(|s| s.id == citation.session_id)
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "session-not-found",
                format!(
                    "session '{}' not found in provider '{}'",
                    citation.session_id, citation.provider.slug()
                ),
            )
            .with_hint("Run `aghist --list` to see available session IDs.")
        })?;

    let messages = provider.load_messages(session).map_err(|e| {
        ErrorEnvelope::new(
            "provider-error",
            format!("failed to load messages for {}: {e}", session.id.0),
        )
    })?;

    let total = messages.len();
    let turn = citation.turn as usize;
    if turn > total {
        return Err(ErrorEnvelope::new(
            "session-not-found",
            format!("turn {turn} out of range: session has {total} message(s)"),
        )
        .with_hint("Use `aghist export` to inspect the full session, or pick a smaller turn."));
    }

    let target_idx = turn - 1; // turn is 1-based, idx is 0-based
    let ctx = include_context as usize;
    let start_idx = target_idx.saturating_sub(ctx);
    let end_idx = (target_idx + ctx + 1).min(total);
    let slice = &messages[start_idx..end_idx];

    let stdout = io::stdout();
    let mut out = stdout.lock();
    match format {
        ShowFormat::Md => render_show_md(&mut out, &citation, session, slice, start_idx, target_idx),
        ShowFormat::Json => render_show_json(&mut out, &citation, session, slice, start_idx, target_idx),
        ShowFormat::Text => render_show_text(&mut out, &citation, slice, start_idx, target_idx),
    }
    .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to write show output: {e}")))?;

    Ok(EXIT_OK)
}

fn render_show_md<W: io::Write>(
    out: &mut W,
    citation: &CitationRef,
    session: &Session,
    slice: &[Message],
    start_idx: usize,
    target_idx: usize,
) -> io::Result<()> {
    writeln!(out, "# {citation}")?;
    if let Some(project) = &session.project_name {
        writeln!(out, "_{project}_")?;
    }
    writeln!(out)?;
    for (i, msg) in slice.iter().enumerate() {
        let turn_no = start_idx + i + 1;
        let marker = if start_idx + i == target_idx { " ←" } else { "" };
        writeln!(out, "## Turn {turn_no} — {}{marker}\n", msg.role)?;
        write_blocks_text(out, msg)?;
        writeln!(out)?;
    }
    Ok(())
}

fn render_show_json<W: io::Write>(
    out: &mut W,
    citation: &CitationRef,
    session: &Session,
    slice: &[Message],
    start_idx: usize,
    target_idx: usize,
) -> io::Result<()> {
    #[derive(serde::Serialize)]
    struct ShowMessage<'a> {
        turn: usize,
        is_target: bool,
        role: Role,
        content: &'a [aghist::model::ContentBlock],
        timestamp: chrono::DateTime<chrono::Utc>,
    }
    #[derive(serde::Serialize)]
    struct ShowOut<'a> {
        #[serde(rename = "ref")]
        reference: String,
        provider: Provider,
        session_id: &'a str,
        project: Option<&'a str>,
        target_turn: u32,
        messages: Vec<ShowMessage<'a>>,
    }

    let messages: Vec<ShowMessage> = slice
        .iter()
        .enumerate()
        .map(|(i, m)| ShowMessage {
            turn: start_idx + i + 1,
            is_target: start_idx + i == target_idx,
            role: m.role,
            content: &m.content,
            timestamp: m.timestamp,
        })
        .collect();

    let payload = ShowOut {
        reference: citation.to_string(),
        provider: citation.provider,
        session_id: session.id.0.as_str(),
        project: session.project_name.as_deref(),
        target_turn: citation.turn,
        messages,
    };

    serde_json::to_writer(&mut *out, &payload).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_show_text<W: io::Write>(
    out: &mut W,
    citation: &CitationRef,
    slice: &[Message],
    start_idx: usize,
    target_idx: usize,
) -> io::Result<()> {
    writeln!(out, "{citation}")?;
    for (i, msg) in slice.iter().enumerate() {
        let turn_no = start_idx + i + 1;
        let marker = if start_idx + i == target_idx { " (target)" } else { "" };
        writeln!(out, "--- Turn {turn_no} — {}{marker} ---", msg.role)?;
        write_blocks_text(out, msg)?;
    }
    Ok(())
}

fn write_blocks_text<W: io::Write>(out: &mut W, msg: &Message) -> io::Result<()> {
    use aghist::model::ContentBlock;
    for block in &msg.content {
        match block {
            ContentBlock::Text(t) => writeln!(out, "{t}")?,
            ContentBlock::CodeBlock { language, code } => {
                let lang = language.as_deref().unwrap_or("");
                writeln!(out, "```{lang}\n{code}\n```")?;
            }
            ContentBlock::ToolUse(tool) => {
                writeln!(out, "[tool: {}]\n{}", tool.name, tool.arguments)?;
            }
            ContentBlock::ToolResult(result) => {
                let status = if result.success { "ok" } else { "err" };
                writeln!(out, "[tool-result {status}]\n{}", result.output)?;
            }
            ContentBlock::Thinking(t) => writeln!(out, "[thinking] {t}")?,
            ContentBlock::Error(t) => writeln!(out, "[error] {t}")?,
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn decisions_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    session_filter: Option<&str>,
    threshold: f32,
    limit: usize,
    force_json: bool,
    filters: &FilterArgs,
) -> Result<i32, ErrorEnvelope> {
    if !threshold.is_finite() || threshold < 0.0 {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("--threshold must be a non-negative finite number (got {threshold})"),
        ));
    }

    // If --session was given as a full citation ref, drop the trailing
    // `#turn` so it can match the session id; we extract decisions across
    // the whole session regardless of the cited turn.
    let session_needle = session_filter.map(|s| {
        let trimmed = s.trim();
        let without_turn = trimmed.rsplit_once('#').map_or(trimmed, |(head, _)| head);
        // Strip leading provider segment if present (`<slug>/<id>` → `<id>`).
        without_turn
            .split_once('/')
            .map_or(without_turn, |(_, rest)| rest)
            .to_string()
    });

    let project_needle = filters
        .project
        .as_deref()
        .map(str::to_lowercase)
        .filter(|s| !s.is_empty());

    let mut rows: Vec<DecisionRow> = Vec::new();

    for p in providers {
        if let Some(want) = filters.provider {
            if p.provider() != want {
                continue;
            }
        }
        let sessions = match p.discover_sessions() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: error: {e}", p.provider());
                continue;
            }
        };
        for session in sessions {
            if !session_matches(&session, filters, project_needle.as_deref()) {
                continue;
            }
            if let Some(needle) = session_needle.as_deref() {
                if !session.id.0.starts_with(needle) {
                    continue;
                }
            }
            // skip corrupt/unreadable, like the rest of the pipeline
            let Ok(messages) = p.load_messages(&session) else {
                continue;
            };

            // Apply per-message filters (--role, --has-tool-call) before
            // running extraction. We track the original turn index so the
            // citation ref still matches the on-disk message position.
            let scored: Vec<(usize, &Message)> = messages
                .iter()
                .enumerate()
                .filter(|(_, m)| message_matches(m, filters))
                .collect();

            for (idx, msg) in scored {
                let turn = u32::try_from(idx + 1).unwrap_or(u32::MAX);
                let cands = aghist::decisions::extract_from_message(msg, turn, threshold);
                for c in cands {
                    let citation = aghist::model::CitationRef::new(
                        session.provider,
                        session.id.clone(),
                        c.turn,
                    );
                    let Some(citation) = citation else { continue };
                    rows.push(DecisionRow {
                        citation,
                        candidate: c,
                        project: session.project_name.clone(),
                        started_at: session.started_at,
                    });
                }
            }
        }
    }

    rows.sort_by(|a, b| {
        b.candidate
            .score
            .partial_cmp(&a.candidate.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.started_at.cmp(&a.started_at))
            .then_with(|| a.citation.session_id.0.cmp(&b.citation.session_id.0))
            .then_with(|| a.candidate.turn.cmp(&b.candidate.turn))
    });
    if rows.len() > limit {
        rows.truncate(limit);
    }

    if rows.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    let want_json = force_json || !io::stdout().is_terminal();
    if want_json {
        print_decisions_json(&rows).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?;
    } else {
        print_decisions_table(&rows);
    }
    Ok(EXIT_OK)
}

struct DecisionRow {
    citation: aghist::model::CitationRef,
    candidate: aghist::decisions::DecisionCandidate,
    project: Option<String>,
    started_at: DateTime<Utc>,
}

fn print_decisions_table(rows: &[DecisionRow]) {
    println!(
        "{:<6}  {:<36}  {:<24}  SNIPPET",
        "SCORE", "REF", "MARKERS"
    );
    for row in rows {
        let r = row.citation.to_string();
        let r = truncate(&r, 36);
        let markers = row.candidate.markers.join(",");
        let markers = truncate(&markers, 24);
        let snippet = truncate(&row.candidate.snippet, 80);
        println!(
            "{:<6.2}  {:<36}  {:<24}  {}",
            row.candidate.score, r, markers, snippet
        );
    }
}

fn print_decisions_json(rows: &[DecisionRow]) -> std::io::Result<()> {
    #[derive(serde::Serialize)]
    struct JsonRow<'a> {
        #[serde(rename = "ref")]
        reference: String,
        provider: aghist::model::Provider,
        session_id: &'a str,
        turn: u32,
        role: aghist::model::Role,
        score: f32,
        markers: &'a [String],
        snippet: &'a str,
        project: Option<&'a str>,
        timestamp: chrono::DateTime<chrono::Utc>,
        started_at: chrono::DateTime<chrono::Utc>,
    }

    let payload: Vec<JsonRow> = rows
        .iter()
        .map(|r| JsonRow {
            reference: r.citation.to_string(),
            provider: r.citation.provider,
            session_id: r.citation.session_id.0.as_str(),
            turn: r.citation.turn,
            role: r.candidate.role,
            score: r.candidate.score,
            markers: &r.candidate.markers,
            snippet: r.candidate.snippet.as_str(),
            project: r.project.as_deref(),
            timestamp: r.candidate.timestamp,
            started_at: r.started_at,
        })
        .collect();

    serde_json::to_writer(io::stdout().lock(), &payload).map_err(std::io::Error::other)?;
    println!();
    Ok(())
}
