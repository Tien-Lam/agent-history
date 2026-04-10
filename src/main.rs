use aghist::{app, config, export, provider, search};

use std::io;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

#[derive(Parser)]
#[command(name = "aghist", about = "Browse and search AI agent conversation history")]
struct Cli {
    /// List sessions without opening the TUI
    #[arg(long)]
    list: bool,

    /// Force rebuild the search index
    #[arg(long)]
    reindex: bool,

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
}

fn main() -> anyhow::Result<()> {
    color_eyre::install().ok();

    let cli = Cli::parse();

    if cli.reindex {
        let index_dir = search::SearchIndex::default_index_dir();
        if let Ok(index) = search::SearchIndex::open_or_create(&index_dir) {
            let _ = index.clear();
            eprintln!("Search index cleared. Will rebuild on next launch.");
        }
    }

    let providers = provider::detect_all_providers();

    if let Some(Command::Export {
        format,
        session,
        output,
    }) = cli.command
    {
        return export_session(&providers, format, &session, output.as_deref());
    }

    let config = config::Config::load();

    if cli.list {
        return list_sessions(&providers);
    }

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app
    let mut app = app::App::new(providers, config);
    let result = app.run(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn export_session(
    providers: &[Box<dyn provider::HistoryProvider>],
    format: export::ExportFormat,
    session_id: &str,
    output: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    let mut all_sessions = Vec::new();
    for p in providers {
        if let Ok(sessions) = p.discover_sessions() {
            all_sessions.extend(sessions);
        }
    }

    let session = all_sessions
        .iter()
        .find(|s| s.id.0 == session_id || s.id.0.starts_with(session_id))
        .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;

    let provider = providers
        .iter()
        .find(|p| p.provider() == session.provider)
        .ok_or_else(|| anyhow::anyhow!("Provider not available for session"))?;

    let messages = provider.load_messages(session)?;
    let content = export::export(format, session, &messages);

    if let Some(path) = output {
        std::fs::write(path, &content)?;
        eprintln!("Exported to {}", path.display());
    } else {
        print!("{content}");
    }

    Ok(())
}

#[allow(clippy::unnecessary_wraps)]
fn list_sessions(providers: &[Box<dyn provider::HistoryProvider>]) -> anyhow::Result<()> {
    let mut all_sessions = Vec::new();

    for p in providers {
        match p.discover_sessions() {
            Ok(sessions) => {
                println!("{}: {} sessions", p.provider(), sessions.len());
                all_sessions.extend(sessions);
            }
            Err(e) => {
                eprintln!("{}: error: {e}", p.provider());
            }
        }
    }

    all_sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    println!("\nTotal: {} sessions\n", all_sessions.len());

    for s in all_sessions.iter().take(20) {
        let project = s.project_name.as_deref().unwrap_or("(unknown)");
        let branch = s.git_branch.as_deref().unwrap_or("");
        let summary = s
            .summary
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(60)
            .collect::<String>();
        println!(
            "  {} | {} | {} | {} | {}",
            s.started_at.format("%Y-%m-%d %H:%M"),
            s.provider,
            project,
            branch,
            summary
        );
    }

    Ok(())
}
