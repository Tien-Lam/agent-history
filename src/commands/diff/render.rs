use std::io::{self, Write as _};

use aghist::cli_error::ErrorEnvelope;
use aghist::output::write_json_line;

use super::{algorithm::DiffOp, DiffRenderInput};

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
    .map_err(|e| ErrorEnvelope::io("failed to write diff output", e))?;
    writeln!(
        out,
        "+++ {raw2}  ({}  {} msgs)",
        render.sess2.started_at.format("%Y-%m-%d"),
        render.lines2.len(),
        raw2 = render.raw2,
    )
    .map_err(|e| ErrorEnvelope::io("failed to write diff output", e))?;

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
            .map_err(|e| ErrorEnvelope::io("failed to write diff output", e))?;
        return Ok(());
    }

    let mut hunks: Vec<(usize, usize)> = Vec::new();
    for &c in &changed {
        let start = c.saturating_sub(context);
        let end = c.saturating_add(context).saturating_add(1).min(flat.len());
        if let Some(last) = hunks.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        hunks.push((start, end));
    }

    for (hunk_start, hunk_end) in hunks {
        let Some(hunk) = flat.get(hunk_start..hunk_end) else {
            continue;
        };
        let Some(first) = hunk.first() else {
            continue;
        };
        let a_start = first.side_a.unwrap_or(0) + 1;
        let b_start = first.side_b.unwrap_or(0) + 1;
        let a_count = hunk.iter().filter(|f| f.side_a.is_some()).count();
        let b_count = hunk.iter().filter(|f| f.side_b.is_some()).count();
        writeln!(out, "@@ -{a_start},{a_count} +{b_start},{b_count} @@")
            .map_err(|e| ErrorEnvelope::io("failed to write diff output", e))?;
        for f in hunk {
            let line = match (f.side_a, f.side_b) {
                (Some(a), _) => render.lines1.get(a),
                (None, Some(b)) => render.lines2.get(b),
                _ => continue,
            };
            let Some(line) = line else {
                continue;
            };
            writeln!(out, "{}{}: {}", f.marker, line.role, line.snippet)
                .map_err(|e| ErrorEnvelope::io("failed to write diff output", e))?;
        }
    }
    Ok(())
}

pub(super) fn render_diff_json(render: &DiffRenderInput<'_>) -> Result<(), ErrorEnvelope> {
    let entries: Vec<serde_json::Value> = render
        .ops
        .iter()
        .filter_map(|op| match op {
            DiffOp::Same(a, b) => {
                render
                    .lines1
                    .get(*a)
                    .zip(render.lines2.get(*b))
                    .map(|(line, _)| {
                        serde_json::json!({
                            "op": "same",
                            "role": line.role,
                            "snippet": line.snippet,
                            "turn_a": a + 1,
                            "turn_b": b + 1,
                        })
                    })
            }
            DiffOp::Delete(a) => render.lines1.get(*a).map(|line| {
                serde_json::json!({
                    "op": "delete",
                    "role": line.role,
                    "snippet": line.snippet,
                    "turn_a": a + 1,
                })
            }),
            DiffOp::Insert(b) => render.lines2.get(*b).map(|line| {
                serde_json::json!({
                    "op": "insert",
                    "role": line.role,
                    "snippet": line.snippet,
                    "turn_b": b + 1,
                })
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
    write_json_line(&mut out, &payload)
        .map_err(|e| ErrorEnvelope::io("failed to write diff output", e))?;
    Ok(())
}
