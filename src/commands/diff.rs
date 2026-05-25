use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::Session;
use aghist::output::should_emit_json;
use aghist::services::lookup as lookup_service;
use aghist::session_resolver::SelectorShape;
use aghist::{provider, query_scope};

use super::discovery::federated_discovery_for_commands;

mod algorithm;
mod render;

use algorithm::{diff_lines_for_messages, lcs_diff, DiffLine, DiffOp};
use render::{render_diff_json, render_diff_text};

pub(crate) fn diff_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    raw1: &str,
    raw2: &str,
    context: usize,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    let discovery = federated_discovery_for_commands(providers, scope);
    let target1 = lookup_service::load_session_by_selector(
        providers,
        &discovery,
        raw1,
        SelectorShape::SessionRefOnly,
    )?;
    let target2 = lookup_service::load_session_by_selector(
        providers,
        &discovery,
        raw2,
        SelectorShape::SessionRefOnly,
    )?;

    let lines1 = diff_lines_for_messages(&target1.messages);
    let lines2 = diff_lines_for_messages(&target2.messages);

    let ops = lcs_diff(&lines1, &lines2)?;
    let want_json = should_emit_json(force_json);
    let render = DiffRenderInput {
        raw1: &target1.session_ref,
        raw2: &target2.session_ref,
        sess1: &target1.session,
        sess2: &target2.session,
        lines1: &lines1,
        lines2: &lines2,
        ops: &ops,
    };

    if want_json {
        render_diff_json(&render)?;
    } else {
        render_diff_text(&render, context)?;
    }

    let has_changes = ops.iter().any(|o| !matches!(o, DiffOp::Same(_, _)));
    Ok(if has_changes { EXIT_OK } else { EXIT_EMPTY })
}

struct DiffRenderInput<'a> {
    raw1: &'a str,
    raw2: &'a str,
    sess1: &'a Session,
    sess2: &'a Session,
    lines1: &'a [DiffLine],
    lines2: &'a [DiffLine],
    ops: &'a [DiffOp],
}
