use anyhow::Result;
use clap::Parser;

use agent_history::cli::{Cli, Command};
use agent_history::commands;
use agent_history::provider::registry::Registry;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let registry = Registry::default();

    match cli.command {
        Command::List(args) => commands::list::run(&args, &registry),
        Command::Show(args) => commands::show::run(&args, &registry),
        Command::Search(args) => commands::search::run(&args, &registry),
    }
}
