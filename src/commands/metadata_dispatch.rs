use aghist::cli_error::ErrorEnvelope;
use aghist::output::OutputMode;
use aghist::provider;

use super::super::cli::{Command, SourcesCommand};
use super::health::health_command;
use super::metadata::{note_dispatch, star_command, stars_list, tag_dispatch, unstar_command};
use super::sources::{
    sources_add_remote, sources_command, sources_list_remote, sources_pull_remote,
    sources_remove_remote,
};

pub(crate) fn dispatch_metadata_command(
    command: Command,
    providers: &[Box<dyn provider::HistoryProvider>],
    one_shot_mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    match command {
        Command::Sources { command } => dispatch_sources_command(command, providers, one_shot_mode),
        Command::Health => health_command(providers, one_shot_mode),
        Command::Note { command } => note_dispatch(command, one_shot_mode),
        Command::Tag { command } => tag_dispatch(command, one_shot_mode),
        Command::Star { reference } => star_command(&reference, one_shot_mode),
        Command::Unstar { reference } => unstar_command(&reference, one_shot_mode),
        Command::Stars { reference, json } => {
            let mode = if json {
                OutputMode::Json
            } else {
                one_shot_mode
            };
            stars_list(reference.as_deref(), mode)
        }
        _ => unreachable!("metadata dispatch received unrelated command"),
    }
}

fn dispatch_sources_command(
    command: Option<SourcesCommand>,
    providers: &[Box<dyn provider::HistoryProvider>],
    mode: OutputMode,
) -> Result<i32, ErrorEnvelope> {
    match command {
        None => sources_command(providers, mode),
        Some(SourcesCommand::List) => sources_list_remote(mode),
        Some(SourcesCommand::Add {
            name,
            host,
            path,
            transport,
        }) => sources_add_remote(&name, &host, &path, transport, mode),
        Some(SourcesCommand::Remove { name }) => sources_remove_remote(&name, mode),
        Some(SourcesCommand::Pull { name, all, dry_run }) => {
            sources_pull_remote(name.as_deref(), all, dry_run, mode)
        }
    }
}
