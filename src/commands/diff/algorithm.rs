use aghist::cli_error::ErrorEnvelope;
use aghist::model::{ContentBlock, Message, Role};

use crate::commands::text::truncate;

const MAX_DIFF_MATRIX_CELLS: usize = 4_000_000;

pub(super) struct DiffLine {
    key: String,
    pub(super) role: String,
    pub(super) snippet: String,
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

pub(super) fn diff_lines_for_messages(messages: &[Message]) -> Vec<DiffLine> {
    messages.iter().map(DiffLine::from_message).collect()
}

fn first_text_snippet(msg: &Message, max: usize) -> String {
    for block in &msg.content {
        if let ContentBlock::Text(text) = block {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return truncate(trimmed, max);
            }
        }
    }
    String::new()
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum DiffOp {
    Same(usize, usize),
    Delete(usize),
    Insert(usize),
}

pub(super) fn lcs_diff(
    left: &[DiffLine],
    right: &[DiffLine],
) -> Result<Vec<DiffOp>, ErrorEnvelope> {
    let rows = left.len();
    let cols = right.len();
    ensure_diff_size(rows, cols)?;
    let width = cols + 1;
    let mut dp = vec![0usize; diff_matrix_cells(rows, cols).unwrap_or(0)];
    for row in (0..rows).rev() {
        for col in (0..cols).rev() {
            let value = if lines_match(left, right, row, col) {
                dp_value(&dp, width, row + 1, col + 1).saturating_add(1)
            } else {
                dp_value(&dp, width, row + 1, col).max(dp_value(&dp, width, row, col + 1))
            };
            if let Some(cell) = dp.get_mut(dp_index(width, row, col)) {
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
            && (col >= cols
                || dp_value(&dp, width, row + 1, col) >= dp_value(&dp, width, row, col + 1))
        {
            ops.push(DiffOp::Delete(row));
            row += 1;
        } else {
            ops.push(DiffOp::Insert(col));
            col += 1;
        }
    }
    Ok(ops)
}

fn lines_match(left: &[DiffLine], right: &[DiffLine], row: usize, col: usize) -> bool {
    left.get(row)
        .zip(right.get(col))
        .is_some_and(|(left, right)| left.key == right.key)
}

fn dp_value(dp: &[usize], width: usize, row: usize, col: usize) -> usize {
    dp.get(dp_index(width, row, col)).copied().unwrap_or(0)
}

fn dp_index(width: usize, row: usize, col: usize) -> usize {
    row.saturating_mul(width).saturating_add(col)
}

fn ensure_diff_size(rows: usize, cols: usize) -> Result<(), ErrorEnvelope> {
    let Some(cells) = diff_matrix_cells(rows, cols) else {
        return Err(diff_too_large_error(rows, cols));
    };
    if cells > MAX_DIFF_MATRIX_CELLS {
        return Err(diff_too_large_error(rows, cols));
    }
    Ok(())
}

fn diff_matrix_cells(rows: usize, cols: usize) -> Option<usize> {
    rows.checked_add(1)?.checked_mul(cols.checked_add(1)?)
}

fn diff_too_large_error(rows: usize, cols: usize) -> ErrorEnvelope {
    ErrorEnvelope::new(
        "diff-too-large",
        format!(
            "diff is too large: {rows} turn(s) by {cols} turn(s) exceeds {MAX_DIFF_MATRIX_CELLS} comparison cells"
        ),
    )
    .with_hint("Use `aghist show --range` or narrower sessions before diffing very large histories.")
}

#[cfg(test)]
mod tests {
    use super::{ensure_diff_size, lcs_diff, DiffLine, DiffOp};

    fn line(key: &str) -> DiffLine {
        DiffLine {
            key: key.to_string(),
            role: "user".to_string(),
            snippet: key.to_string(),
        }
    }

    #[test]
    fn lcs_diff_orders_delete_before_insert_for_replacements() {
        let left = [line("a"), line("b")];
        let right = [line("a"), line("c")];

        let ops = lcs_diff(&left, &right).unwrap();

        assert_eq!(
            ops,
            vec![DiffOp::Same(0, 0), DiffOp::Delete(1), DiffOp::Insert(1)]
        );
    }

    #[test]
    fn diff_size_guard_rejects_quadratic_allocations() {
        let err = ensure_diff_size(2_000, 2_000).unwrap_err();

        assert_eq!(err.kind, "diff-too-large");
    }
}
