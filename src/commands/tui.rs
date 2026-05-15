use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::{app, config, provider};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub(crate) fn run_tui(
    providers: Vec<Box<dyn provider::HistoryProvider>>,
    config: config::Config,
) -> Result<i32, ErrorEnvelope> {
    // Install panic hook that restores the terminal before printing the panic.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));

    enable_raw_mode()
        .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to enable raw mode: {e}")))?;
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
