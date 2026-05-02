use aghist::cli_error::{
    ErrorEnvelope, EXIT_EMPTY, EXIT_ERROR, EXIT_OK, EXIT_USAGE,
};
use aghist::model::{Provider, Session};
use aghist::output::{CommandKind, OutputMode};
use aghist::{app, config, export, provider, search};

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
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

    #[command(subcommand)]
    command: Option<Command>,
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
    },
    /// Search indexed sessions for a query
    Search {
        /// Tantivy query string (matches content + project fields)
        query: String,

        /// Maximum number of hits to return
        #[arg(long, short = 'n', default_value_t = 20)]
        limit: usize,

        /// Force JSON output (default: JSON on pipe, table on TTY)
        #[arg(long)]
        json: bool,
    },
    /// Update aghist to the latest release
    Update,
    /// Remove aghist binary and data
    Uninstall,
}

fn parse_provider_slug(raw: &str) -> Result<Provider, String> {
    Provider::from_slug(raw).ok_or_else(|| {
        format!(
            "unknown provider slug '{raw}'. Valid: claude-code, copilot-cli, gemini-cli, codex-cli, opencode"
        )
    })
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

fn run(cli: Cli) -> Result<i32, ErrorEnvelope> {
    if cli.reindex {
        let index_dir = search::SearchIndex::default_index_dir();
        if let Ok(index) = search::SearchIndex::open_or_create(&index_dir) {
            let _ = index.clear();
            eprintln!("Search index cleared. Will rebuild on next launch.");
        }
    }

    let config = config::Config::load();
    let enabled = config.enabled_providers();
    let providers: Vec<_> = provider::detect_all_providers()
        .into_iter()
        .filter(|p| enabled.contains(&p.provider()))
        .collect();

    match cli.command {
        Some(Command::Update) => return self_update(),
        Some(Command::Uninstall) => return uninstall(),
        Some(Command::Export {
            format,
            session,
            output,
        }) => return export_session(&providers, format, &session, output.as_deref()),
        Some(Command::Index { provider, force }) => {
            return run_index(&providers, provider, force);
        }
        Some(Command::Search {
            query,
            limit,
            json,
        }) => return search_command(&providers, &query, limit, json),
        None => {}
    }

    if cli.json && cli.ndjson {
        ErrorEnvelope::new("usage", "--json and --ndjson are mutually exclusive")
            .with_hint("Pick one. Without either, output auto-detects: JSON/NDJSON on a pipe, human format on a TTY.")
            .emit();
        return Ok(EXIT_USAGE);
    }

    if cli.list {
        let mode = OutputMode::resolve(cli.json, cli.ndjson, CommandKind::Streaming);
        return list_sessions(&providers, mode);
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

fn run_index(
    providers: &[Box<dyn provider::HistoryProvider>],
    filter: Option<Provider>,
    force: bool,
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
    });

    println!("{summary}");
    Ok(EXIT_OK)
}

fn export_session(
    providers: &[Box<dyn provider::HistoryProvider>],
    format: export::ExportFormat,
    session_id: &str,
    output: Option<&std::path::Path>,
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
    let content = export::export(format, session, &messages);

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

fn search_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    query: &str,
    limit: usize,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    use aghist::model::Session;

    if query.trim().is_empty() {
        ErrorEnvelope::new("usage", "search query is empty")
            .with_hint("Run `aghist search --help` for usage.")
            .emit();
        return Ok(EXIT_USAGE);
    }

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

    let hits = index
        .search(query, limit)
        .map_err(|e| ErrorEnvelope::new("index-error", format!("search failed: {e}")))?;

    if hits.is_empty() {
        return Ok(EXIT_EMPTY);
    }

    // Tie-break by (started_at DESC, session_id ASC) for deterministic ordering.
    // Tantivy already returns score-DESC; we use a stable sort to preserve that
    // and only reorder ties.
    let session_meta: std::collections::HashMap<&str, &Session> =
        sessions.iter().map(|s| (s.id.0.as_str(), s)).collect();

    let mut ordered: Vec<search::SearchHit> = hits;
    ordered.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_started = session_meta.get(a.session_id.as_str()).map(|s| s.started_at);
                let b_started = session_meta.get(b.session_id.as_str()).map(|s| s.started_at);
                b_started.cmp(&a_started)
            })
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    let want_json = force_json || !io::stdout().is_terminal();

    if want_json {
        print_search_json(&ordered, &session_meta).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?;
    } else {
        print_search_table(&ordered, &session_meta);
    }

    Ok(EXIT_OK)
}

fn print_search_json(
    hits: &[search::SearchHit],
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

    let rows: Vec<JsonHit> = hits
        .iter()
        .map(|h| {
            let session = sessions.get(h.session_id.as_str()).copied();
            JsonHit {
                session_id: &h.session_id,
                message_id: &h.message_id,
                score: h.score,
                snippet: &h.snippet,
                provider: session.map(|s| s.provider),
                project: session.and_then(|s| s.project_name.as_deref()),
                started_at: session.map(|s| s.started_at),
            }
        })
        .collect();

    serde_json::to_writer(io::stdout().lock(), &rows)?;
    println!();
    Ok(())
}

fn print_search_table(
    hits: &[search::SearchHit],
    sessions: &std::collections::HashMap<&str, &aghist::model::Session>,
) {
    println!(
        "{:<6}  {:<16}  {:<12}  {:<20}  {:<14}  SNIPPET",
        "SCORE", "STARTED", "PROVIDER", "PROJECT", "SESSION"
    );
    for h in hits {
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
    }
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

#[allow(clippy::unnecessary_wraps)]
fn list_sessions(
    providers: &[Box<dyn provider::HistoryProvider>],
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    let mut all_sessions = Vec::new();

    for p in providers {
        match p.discover_sessions() {
            Ok(sessions) => {
                if !mode.is_machine() {
                    println!("{}: {} sessions", p.provider(), sessions.len());
                }
                all_sessions.extend(sessions);
            }
            Err(e) => {
                eprintln!("{}: error: {e}", p.provider());
            }
        }
    }

    all_sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    match mode {
        OutputMode::Human => render_list_human(&all_sessions),
        OutputMode::Json => render_list_json(&all_sessions).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write JSON output: {e}"))
        })?,
        OutputMode::Ndjson => render_list_ndjson(&all_sessions).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write NDJSON output: {e}"))
        })?,
    }

    if all_sessions.is_empty() {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

fn render_list_human(sessions: &[Session]) {
    println!("\nTotal: {} sessions\n", sessions.len());
    for s in sessions.iter().take(20) {
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

fn render_list_json(sessions: &[Session]) -> std::io::Result<()> {
    use std::io::Write as _;
    let rows: Vec<SessionRow<'_>> = sessions.iter().map(SessionRow::from_session).collect();
    let doc = serde_json::json!({ "sessions": rows });
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &doc).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}

fn render_list_ndjson(sessions: &[Session]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    for s in sessions {
        let row = SessionRow::from_session(s);
        serde_json::to_writer(&mut out, &row).map_err(std::io::Error::other)?;
        writeln!(out)?;
    }
    Ok(())
}
