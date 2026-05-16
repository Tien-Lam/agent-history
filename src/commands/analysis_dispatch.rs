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
        Command::Track {
            topic,
            limit,
            json,
            llm_model,
        } => track_command(
            providers,
            filters,
            &topic,
            limit,
            json,
            llm_model.as_deref(),
        ),
        Command::Decisions {
            session,
            threshold,
            limit,
            json,
            llm,
            llm_model,
        } => decisions_command(
            providers,
            DecisionsCommandRequest {
                session_filter: session.as_deref(),
                threshold,
                limit,
                force_json: json,
                filters,
                use_llm: llm,
                llm_model: llm_model.as_deref(),
            },
        ),
        Command::Todos {
            kind,
            limit,
            json,
            llm,
            llm_model,
        } => todos_command(
            providers,
            filters,
            &kind,
            limit,
            json,
            llm,
            llm_model.as_deref(),
        ),
        Command::Threads {
            gap_hours,
            min_sessions,
            limit,
            json,
            llm,
            llm_model,
            llm_max_sessions,
        } => threads_command(
            providers,
            ThreadsCommandRequest {
                filters,
                gap_hours,
                min_sessions,
                limit,
                force_json: json,
                use_llm: llm,
                llm_model: llm_model.as_deref(),
                llm_max_sessions,
            },
        ),
        _ => unreachable!("analysis dispatch received unrelated command"),
    }
}
