use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::{ContentBlock, Message, Role, Session};
use aghist::services::lookup as lookup_service;
use aghist::session_resolver::SelectorShape;
use aghist::{provider, query_scope};

use super::discovery::federated_discovery_for_commands;
use super::text::truncate;

mod render;

use render::{render_diff_json, render_diff_text};

struct DiffLine {
    key: String,
    role: String,
    snippet: String,
}

impl DiffLine {
    fn from_message(msg: &Message) -> Self {
        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Tool => "tool",
        };
        let snippet = first_text_snippet(msg, 120);
        let key = format!("{role}:{}", first_text_snippet(msg, 64));
        Self {
            key,
            role: role.to_string(),
            snippet,
        }
    }
}

fn first_text_snippet(msg: &Message, max: usize) -> String {
    for block in &msg.content {
        if let ContentBlock::Text(t) = block {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                return truncate(trimmed, max);
            }
        }
    }
    String::new()
}

enum DiffOp {
    Same(usize, usize),
    Delete(usize),
    Insert(usize),
}

fn lcs_diff(left: &[DiffLine], right: &[DiffLine]) -> Vec<DiffOp> {
    let rows = left.len();
    let cols = right.len();
    let mut dp = vec![vec![0usize; cols + 1]; rows + 1];
    for row in (0..rows).rev() {
        for col in (0..cols).rev() {
            let value = if lines_match(left, right, row, col) {
                dp_value(&dp, row + 1, col + 1).saturating_add(1)
            } else {
                dp_value(&dp, row + 1, col).max(dp_value(&dp, row, col + 1))
            };
            if let Some(cell) = dp.get_mut(row).and_then(|line| line.get_mut(col)) {
                *cell = value;
            }
        }
    }
    let mut ops = Vec::new();
    let (mut row, mut col) = (0, 0);
    while row < rows || col < cols {
        if row < rows && col < cols && lines_match(left, right, row, col) {
            ops.push(DiffOp::Same(row, col));
            row += 1;
            col += 1;
        } else if row < rows
            && (col >= cols || dp_value(&dp, row + 1, col) >= dp_value(&dp, row, col + 1))
        {
            ops.push(DiffOp::Delete(row));
            row += 1;
        } else {
            ops.push(DiffOp::Insert(col));
            col += 1;
        }
    }
    ops
}

fn lines_match(left: &[DiffLine], right: &[DiffLine], row: usize, col: usize) -> bool {
    left.get(row)
        .zip(right.get(col))
        .is_some_and(|(left, right)| left.key == right.key)
}

fn dp_value(dp: &[Vec<usize>], row: usize, col: usize) -> usize {
    dp.get(row)
        .and_then(|line| line.get(col))
        .copied()
        .unwrap_or(0)
}

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

    let lines1: Vec<DiffLine> = target1
        .messages
        .iter()
        .map(DiffLine::from_message)
        .collect();
    let lines2: Vec<DiffLine> = target2
        .messages
        .iter()
        .map(DiffLine::from_message)
        .collect();

    let ops = lcs_diff(&lines1, &lines2);
    let want_json = force_json || !io::stdout().is_terminal();
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
