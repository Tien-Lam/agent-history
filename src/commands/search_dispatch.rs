use std::collections::HashSet;
use std::path::PathBuf;

use aghist::cli_error::ErrorEnvelope;
use aghist::search::SearchFilters;
use aghist::{provider, query_scope};

use super::super::cli::{resolve_search_args, SearchArgs};
use super::context::CommandContext;
use super::search::{
    search_command, search_watch_command, SearchCommandRequest, SearchWatchRequest,
};

pub(crate) struct SearchDispatchArgs {
    pub(crate) query: Option<String>,
    pub(crate) query_file: Option<PathBuf>,
    pub(crate) stdin: bool,
    pub(crate) limit: usize,
    pub(crate) cursor: Option<String>,
    pub(crate) json: bool,
    pub(crate) hybrid_weight: f32,
    pub(crate) params: Option<String>,
    pub(crate) mode: SearchDispatchMode,
}

#[derive(Clone, Copy)]
pub(crate) enum SearchDispatchMode {
    Once { debug_search: bool },
    Watch { interval_ms: u64, iterations: u32 },
}

pub(crate) fn dispatch_search_command(
    ctx: &CommandContext,
    args: SearchDispatchArgs,
) -> Result<i32, ErrorEnvelope> {
    let filter_args = ctx.filters();
    let filters = filter_args.to_search_filters();
    let metadata_keys = ctx.metadata_filter_keys()?;
    match args.mode {
        SearchDispatchMode::Watch {
            interval_ms,
            iterations,
        } => search_watch_command(
            ctx.providers(),
            ctx.scope(),
            SearchWatchRequest {
                query: args.query.as_deref(),
                query_file: args.query_file.as_deref(),
                stdin: args.stdin,
                limit: args.limit,
                interval_ms,
                max_iterations: iterations,
                filters: &filters,
                metadata_keys: metadata_keys.as_ref(),
            },
        ),
        SearchDispatchMode::Once { debug_search } => dispatch_one_shot_search(
            ctx,
            ctx.providers(),
            ctx.scope(),
            args,
            &filters,
            metadata_keys.as_ref(),
            debug_search,
        ),
    }
}

fn dispatch_one_shot_search(
    ctx: &CommandContext,
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    args: SearchDispatchArgs,
    filters: &SearchFilters,
    metadata_keys: Option<&HashSet<String>>,
    debug_search: bool,
) -> Result<i32, ErrorEnvelope> {
    let resolved = resolve_search_args(
        SearchArgs {
            query: args.query,
            query_file: args.query_file,
            stdin: args.stdin,
            limit: args.limit,
            cursor: args.cursor,
            json: args.json,
            debug_search,
            hybrid_weight: args.hybrid_weight,
        },
        args.params,
    )?;
    let debug_search = debug_search || resolved.debug_search;
    let force_json = ctx.json_only_output(resolved.json, "search")?;
    search_command(
        providers,
        scope,
        SearchCommandRequest {
            query: resolved.query.as_deref(),
            query_file: resolved.query_file.as_deref(),
            stdin: resolved.stdin,
            limit: resolved.limit,
            cursor: resolved.cursor.as_deref(),
            force_json,
            filters,
            debug_search,
            hybrid_weight: resolved.hybrid_weight,
            metadata_keys,
        },
    )
}
