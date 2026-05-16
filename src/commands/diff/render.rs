use std::io::{self, Write as _};

use aghist::cli_error::ErrorEnvelope;

use super::{DiffOp, DiffRenderInput};

struct FlatOp {
    marker: char,
    side_a: Option<usize>,
    side_b: Option<usize>,
}

pub(super) fn render_diff_text(
    render: &DiffRenderInput<'_>,
    context: usize,
) -> Result<(), ErrorEnvelope> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(
        out,
        "--- {raw1}  ({}  {} msgs)",
        render.sess1.started_at.format("%Y-%m-%d"),
        render.lines1.len(),
        raw1 = render.raw1,
    )
    .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
    writeln!(
        out,
        "+++ {raw2}  ({}  {} msgs)",
        render.sess2.started_at.format("%Y-%m-%d"),
        render.lines2.len(),
        raw2 = render.raw2,
    )
    .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;

    let flat: Vec<FlatOp> = render
        .ops
        .iter()
        .map(|op| match op {
            DiffOp::Same(a, b) => FlatOp {
                marker: ' ',
                side_a: Some(*a),
                side_b: Some(*b),
            },
            DiffOp::Delete(a) => FlatOp {
                marker: '-',
                side_a: Some(*a),
                side_b: None,
            },
            DiffOp::Insert(b) => FlatOp {
                marker: '+',
                side_a: None,
                side_b: Some(*b),
            },
        })
        .collect();

    let changed: Vec<usize> = flat
        .iter()
        .enumerate()
        .filter(|(_, f)| f.marker != ' ')
        .map(|(i, _)| i)
        .collect();

    if changed.is_empty() {
        writeln!(out, "(sessions are identical)")
            .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
        return Ok(());
    }

    let mut hunks: Vec<(usize, usize)> = Vec::new();
    for &c in &changed {
        let start = c.saturating_sub(context);
        let end = (c + context + 1).min(flat.len());
        if let Some(last) = hunks.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        hunks.push((start, end));
    }

    for (hunk_start, hunk_end) in hunks {
        let a_start = flat[hunk_start].side_a.unwrap_or(0) + 1;
        let b_start = flat[hunk_start].side_b.unwrap_or(0) + 1;
        let a_count = flat[hunk_start..hunk_end]
            .iter()
            .filter(|f| f.side_a.is_some())
            .count();
        let b_count = flat[hunk_start..hunk_end]
            .iter()
            .filter(|f| f.side_b.is_some())
            .count();
        writeln!(out, "@@ -{a_start},{a_count} +{b_start},{b_count} @@")
            .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
        for f in &flat[hunk_start..hunk_end] {
            let line = match (f.side_a, f.side_b) {
                (Some(a), _) => &render.lines1[a],
                (None, Some(b)) => &render.lines2[b],
                _ => continue,
            };
            writeln!(out, "{}{}: {}", f.marker, line.role, line.snippet)
                .map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
        }
    }
    Ok(())
}

pub(super) fn render_diff_json(render: &DiffRenderInput<'_>) -> Result<(), ErrorEnvelope> {
    let entries: Vec<serde_json::Value> = render
        .ops
        .iter()
        .map(|op| match op {
            DiffOp::Same(a, b) => serde_json::json!({
                "op": "same",
                "role": render.lines1[*a].role,
                "snippet": render.lines1[*a].snippet,
                "turn_a": a + 1,
                "turn_b": b + 1,
            }),
            DiffOp::Delete(a) => serde_json::json!({
                "op": "delete",
                "role": render.lines1[*a].role,
                "snippet": render.lines1[*a].snippet,
                "turn_a": a + 1,
            }),
            DiffOp::Insert(b) => serde_json::json!({
                "op": "insert",
                "role": render.lines2[*b].role,
                "snippet": render.lines2[*b].snippet,
                "turn_b": b + 1,
            }),
        })
        .collect();

    let payload = serde_json::json!({
        "session1": {
            "ref": render.raw1,
            "started_at": render.sess1.started_at,
            "turns": render.lines1.len()
        },
        "session2": {
            "ref": render.raw2,
            "started_at": render.sess2.started_at,
            "turns": render.lines2.len()
        },
        "ops": entries,
        "changed": render.ops.iter().filter(|o| !matches!(o, DiffOp::Same(_, _))).count(),
        "same": render.ops.iter().filter(|o| matches!(o, DiffOp::Same(_, _))).count(),
    });
    let stdout = io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, &payload)
        .map_err(|e| ErrorEnvelope::new("io-error", format!("json: {e}")))?;
    writeln!(out).map_err(|e| ErrorEnvelope::new("io-error", e.to_string()))?;
    Ok(())
}
