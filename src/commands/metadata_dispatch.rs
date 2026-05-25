use aghist::cli_error::ErrorEnvelope;
use aghist::output::{CommandKind, OutputMode};
use aghist::provider;

use super::super::cli::{MetadataCommand, SourcesCommand};
use super::context::CommandContext;
use super::health::health_command;
use super::metadata::{note_dispatch, star_command, stars_list, tag_dispatch, unstar_command};
use super::sources::{
    sources_add_remote, sources_command, sources_list_remote, sources_pull_remote,
    sources_remove_remote,
};

pub(crate) fn dispatch_metadata_command(
    command: MetadataCommand,
    ctx: &CommandContext,
) -> Result<i32, ErrorEnvelope> {
    let one_shot_mode = ctx.output_mode(CommandKind::OneShot);
    match command {
        MetadataCommand::Sources { command } => {
            dispatch_sources_command(command, ctx.providers(), one_shot_mode)
        }
        MetadataCommand::Health => health_command(ctx.providers(), ctx.scope(), one_shot_mode),
        MetadataCommand::Note { command } => note_dispatch(command, one_shot_mode),
        MetadataCommand::Tag { command } => tag_dispatch(command, one_shot_mode),
        MetadataCommand::Star { reference } => star_command(&reference, one_shot_mode),
        MetadataCommand::Unstar { reference } => unstar_command(&reference, one_shot_mode),
        MetadataCommand::Stars { reference, json } => {
            let mode = ctx.output_mode_with_local_json(CommandKind::OneShot, json)?;
            stars_list(reference.as_deref(), mode)
        }
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
