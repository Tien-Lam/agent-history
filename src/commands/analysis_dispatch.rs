use aghist::cli_error::ErrorEnvelope;
use aghist::provider;

use super::super::cli::{Command, FilterArgs};
use super::analysis::{
    decisions_command, threads_command, todos_command, track_command, DecisionsCommandRequest,
    ThreadsCommandRequest,
};

pub(crate) fn dispatch_analysis_command(
    command: Command,
    providers: &[Box<dyn provider::HistoryProvider>],
    filters: &FilterArgs,
) -> Result<i32, ErrorEnvelope> {
    match command {
        Command::Track(args) => track_command(
            providers,
            filters,
            &args.topic,
            args.limit,
            args.json,
            args.llm_model.as_deref(),
        ),
        Command::Decisions(args) => decisions_command(
            providers,
            DecisionsCommandRequest {
                session_filter: args.session.as_deref(),
                threshold: args.threshold,
                limit: args.limit,
                force_json: args.json,
                filters,
                use_llm: args.llm,
                llm_model: args.llm_model.as_deref(),
            },
        ),
        Command::Todos(args) => todos_command(
            providers,
            filters,
            &args.kind,
            args.limit,
            args.json,
            args.llm,
            args.llm_model.as_deref(),
        ),
        Command::Threads(args) => threads_command(
            providers,
            ThreadsCommandRequest {
                filters,
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
