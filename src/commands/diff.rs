use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::{ContentBlock, Message, Role, Session};
use aghist::provider;

use super::discovery::federated_discovery_for_commands;
use super::session_select::{resolve_session_selector, SelectedSession, SelectorShape};
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
            dp[row][col] = if left[row].key == right[col].key {
                dp[row + 1][col + 1] + 1
            } else {
                dp[row + 1][col].max(dp[row][col + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let (mut row, mut col) = (0, 0);
    while row < rows || col < cols {
        if row < rows && col < cols && left[row].key == right[col].key {
            ops.push(DiffOp::Same(row, col));
            row += 1;
            col += 1;
        } else if row < rows && (col >= cols || dp[row + 1][col] >= dp[row][col + 1]) {
            ops.push(DiffOp::Delete(row));
            row += 1;
        } else {
            ops.push(DiffOp::Insert(col));
            col += 1;
        }
    }
    ops
}

fn load_session_messages(
    providers: &[Box<dyn provider::HistoryProvider>],
    target: &SelectedSession<'_>,
) -> Result<(Session, Vec<Message>), ErrorEnvelope> {
    let messages = provider::load_messages_for_session(target.session, providers).map_err(|e| {
        ErrorEnvelope::new(
            "provider-error",
            format!("load {}: {e}", target.session_ref),
        )
    })?;
    Ok((target.session.clone(), messages))
}

pub(crate) fn diff_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    raw1: &str,
    raw2: &str,
    context: usize,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    let discovery = federated_discovery_for_commands(providers);
    let target1 = resolve_session_selector(
        &discovery.sessions,
        &discovery.source_by_session,
        raw1,
        SelectorShape::SessionRefOnly,
    )?;
    let target2 = resolve_session_selector(
        &discovery.sessions,
        &discovery.source_by_session,
        raw2,
        SelectorShape::SessionRefOnly,
    )?;
    let (sess1, msgs1) = load_session_messages(providers, &target1)?;
    let (sess2, msgs2) = load_session_messages(providers, &target2)?;

    let lines1: Vec<DiffLine> = msgs1.iter().map(DiffLine::from_message).collect();
    let lines2: Vec<DiffLine> = msgs2.iter().map(DiffLine::from_message).collect();

    let ops = lcs_diff(&lines1, &lines2);
    let want_json = force_json || !io::stdout().is_terminal();
    let render = DiffRenderInput {
        raw1: &target1.session_ref,
        raw2: &target2.session_ref,
        sess1: &sess1,
        sess2: &sess2,
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
