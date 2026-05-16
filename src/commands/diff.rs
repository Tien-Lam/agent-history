use std::io::{self, IsTerminal};

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::model::{ContentBlock, Message, Provider, Role, Session};
use aghist::provider;

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
        } else if col < cols && (row >= rows || dp[row + 1][col] >= dp[row][col + 1]) {
            ops.push(DiffOp::Insert(col));
            col += 1;
        } else {
            ops.push(DiffOp::Delete(row));
            row += 1;
        }
    }
    ops
}

fn load_session_messages(
    providers: &[Box<dyn provider::HistoryProvider>],
    raw: &str,
) -> Result<(Session, Vec<Message>), ErrorEnvelope> {
    let (slug, session_id) = raw.split_once('/').ok_or_else(|| {
        ErrorEnvelope::new(
            "usage",
            format!("invalid session ref '{raw}': expected <provider>/<session-id>"),
        )
    })?;
    let provider_kind = Provider::from_slug(slug)
        .ok_or_else(|| ErrorEnvelope::new("usage", format!("unknown provider slug '{slug}'")))?;
    let p = providers
        .iter()
        .find(|p| p.provider() == provider_kind)
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "provider-unavailable",
                format!("provider '{slug}' not detected"),
            )
        })?;
    let sessions = p
        .discover_sessions()
        .map_err(|e| ErrorEnvelope::new("provider-error", format!("discover {slug}: {e}")))?;
    let session = sessions
        .into_iter()
        .find(|s| s.id.0 == session_id || s.id.0.starts_with(session_id))
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "session-not-found",
                format!("session '{session_id}' not found in {slug}"),
            )
        })?;
    let messages = p.load_messages(&session).map_err(|e| {
        ErrorEnvelope::new("provider-error", format!("load {slug}/{session_id}: {e}"))
    })?;
    Ok((session, messages))
}

pub(crate) fn diff_command(
    providers: &[Box<dyn provider::HistoryProvider>],
    raw1: &str,
    raw2: &str,
    context: usize,
    force_json: bool,
) -> Result<i32, ErrorEnvelope> {
    let (sess1, msgs1) = load_session_messages(providers, raw1)?;
    let (sess2, msgs2) = load_session_messages(providers, raw2)?;

    let lines1: Vec<DiffLine> = msgs1.iter().map(DiffLine::from_message).collect();
    let lines2: Vec<DiffLine> = msgs2.iter().map(DiffLine::from_message).collect();

    let ops = lcs_diff(&lines1, &lines2);
    let want_json = force_json || !io::stdout().is_terminal();
    let render = DiffRenderInput {
        raw1,
        raw2,
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
