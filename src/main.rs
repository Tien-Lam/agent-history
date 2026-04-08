mod action;
mod app;
mod event;
mod model;
mod provider;
mod ui;

use std::io;

use clap::Parser;
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
}

fn main() -> anyhow::Result<()> {
    color_eyre::install().ok();

    let cli = Cli::parse();
    let providers = provider::detect_all_providers();

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
    let mut app = app::App::new(providers);
    let result = app.run(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

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
