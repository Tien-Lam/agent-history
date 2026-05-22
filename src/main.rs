use aghist::cli_error::{ErrorEnvelope, EXIT_USAGE};

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod cli;
mod commands;
use cli::Cli;

fn init_tracing() {
    // Log to ~/.aghist/aghist.log — safe for TUI since it doesn't touch stdout/stderr
    let log_dir = directories::BaseDirs::new()
        .map_or_else(|| PathBuf::from("."), |d| d.home_dir().join(".aghist"));
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("aghist=debug"));

    if let Some(file_appender) = tracing_file_appender(&log_dir) {
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_writer(file_appender).with_ansi(false))
            .try_init();
    } else {
        let _ = tracing_subscriber::registry().with(filter).try_init();
    }
}

fn tracing_file_appender(
    log_dir: &std::path::Path,
) -> Option<tracing_appender::rolling::RollingFileAppender> {
    std::fs::create_dir_all(log_dir).ok()?;
    tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("aghist.log")
        .build(log_dir)
        .ok()
}

fn main() -> ExitCode {
    init_tracing();
    color_eyre::install().ok();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => return handle_clap_error(&err),
    };

    match commands::dispatch::run(cli) {
        Ok(code) => exit_code(code),
        Err(env) => {
            let code = env.exit_code();
            env.emit();
            exit_code(code)
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
