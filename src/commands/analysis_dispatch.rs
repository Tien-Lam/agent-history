use aghist::cli_error::ErrorEnvelope;

use super::super::cli::Command;
use super::analysis::{
    decisions_command, threads_command, todos_command, track_command, DecisionsCommandRequest,
    ThreadsCommandRequest, TodosCommandRequest, TrackCommandRequest,
};
use super::context::CommandContext;

pub(crate) fn dispatch_analysis_command(
    command: Command,
    ctx: &CommandContext,
) -> Result<i32, ErrorEnvelope> {
    let filters = ctx.filters();
    let providers = ctx.providers();
    let scope = ctx.scope();
    let metadata_keys = ctx.metadata_filter_keys()?;
    match command {
        Command::Track(args) => track_command(
            providers,
            scope,
            TrackCommandRequest {
                filters,
                metadata_keys: metadata_keys.as_ref(),
                topic: &args.topic,
                limit: args.limit,
                force_json: args.json,
                llm_model: args.llm_model.as_deref(),
            },
        ),
        Command::Decisions(args) => decisions_command(
            providers,
            scope,
            DecisionsCommandRequest {
                session_filter: args.session.as_deref(),
                threshold: args.threshold,
                limit: args.limit,
                force_json: args.json,
                filters,
                metadata_keys: metadata_keys.as_ref(),
                use_llm: args.llm,
                llm_model: args.llm_model.as_deref(),
            },
        ),
        Command::Todos(args) => todos_command(
            providers,
            scope,
            TodosCommandRequest {
                filters,
                metadata_keys: metadata_keys.as_ref(),
                kinds: &args.kind,
                limit: args.limit,
                force_json: args.json,
                use_llm: args.llm,
                llm_model: args.llm_model.as_deref(),
            },
        ),
        Command::Threads(args) => threads_command(
            providers,
            scope,
            ThreadsCommandRequest {
                filters,
                metadata_keys: metadata_keys.as_ref(),
                gap_hours: args.gap_hours,
                min_sessions: args.min_sessions,
                limit: args.limit,
                force_json: args.json,
                use_llm: args.llm,
                llm_model: args.llm_model.as_deref(),
                llm_max_sessions: args.llm_max_sessions,
            },
        ),
        _ => unreachable!("analysis dispatch received unrelated command"),
    }
}
