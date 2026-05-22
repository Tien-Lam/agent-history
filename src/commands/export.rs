use std::io::Write as _;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::metadata::{self as metadata_store, Note};
use aghist::services::lookup as lookup_service;
use aghist::session_resolver::SelectorShape;
use aghist::{export, provider, query_scope};

use super::discovery::federated_discovery_for_commands;
use super::metadata::metadata_error;

/// Parse a 1-based inclusive turn range against a session of `total` messages.
///
/// Accepts `A:B`, `:B`, `A:`, or a bare `A`. Empty halves default to the
/// session bounds (`1` and `total`). Bounds are clamped to the available
/// range so callers can do `--turn-range :999` without failing.
///
/// Returns `(start, end)` with `1 <= start <= end <= total`, ready to be
/// converted to a 0-based half-open slice via `start-1 .. end`.
fn parse_turn_range(spec: &str, total: usize) -> Result<(usize, usize), String> {
    if total == 0 {
        return Err("session has no messages to slice".to_string());
    }
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(format!("empty turn range '{spec}'"));
    }

    let parse_bound = |s: &str, label: &str| -> Result<Option<usize>, String> {
        if s.is_empty() {
            return Ok(None);
        }
        s.parse::<usize>()
            .map(Some)
            .map_err(|_| format!("invalid {label} '{s}' in turn range '{spec}'"))
    };

    let (start_raw, end_raw) = if let Some((a, b)) = trimmed.split_once(':') {
        (parse_bound(a, "start")?, parse_bound(b, "end")?)
    } else {
        let n =
            parse_bound(trimmed, "turn")?.ok_or_else(|| format!("empty turn range '{spec}'"))?;
        (Some(n), Some(n))
    };

    if start_raw == Some(0) || end_raw == Some(0) {
        return Err(format!("turn range '{spec}' uses 0 (turns are 1-based)"));
    }

    let start = start_raw.unwrap_or(1);
    let end = end_raw.unwrap_or(total).min(total);

    if start > total {
        return Err(format!(
            "turn range '{spec}' starts at {start} but session only has {total} message(s)"
        ));
    }
    if end < start {
        return Err(format!(
            "turn range '{spec}' has end ({end}) before start ({start})"
        ));
    }
    Ok((start, end))
}

/// Pull the session's notes from the metadata sidecar, restricted to the
/// exported turn range. Missing sidecar is a no-op; an existing but unreadable
/// or corrupt sidecar is an error because `--include-notes` was explicit.
/// Notes that fall outside the slice are dropped; turn-level notes have their
/// `session_ref` rebased so turn N in the full session becomes turn
/// `N - turn_offset` in the slice.
fn load_session_notes(
    session_ref: &str,
    turn_offset: usize,
    slice_len: usize,
) -> Result<Vec<Note>, ErrorEnvelope> {
    let Some(path) = metadata_store::default_path() else {
        return Ok(Vec::new());
    };
    if !path.exists() {
        return Ok(Vec::new());
    }
    let conn = metadata_store::open(&path).map_err(|e| metadata_error(&e))?;
    let all =
        metadata_store::note_list(&conn, Some(session_ref)).map_err(|e| metadata_error(&e))?;
    let offset = u32::try_from(turn_offset).unwrap_or(u32::MAX);
    let max_turn_inclusive = offset.saturating_add(u32::try_from(slice_len).unwrap_or(u32::MAX));
    let turn_prefix = format!("{session_ref}#");
    let mut out = Vec::with_capacity(all.len());
    for mut n in all {
        if n.session_ref == session_ref {
            out.push(n);
            continue;
        }
        let Some(rest) = n.session_ref.strip_prefix(&turn_prefix) else {
            continue;
        };
        let Ok(turn) = rest.parse::<u32>() else {
            continue;
        };
        if turn == 0 || turn <= offset || turn > max_turn_inclusive {
            continue;
        }
        let rebased = turn - offset;
        n.session_ref = format!("{turn_prefix}{rebased}");
        out.push(n);
    }
    Ok(out)
}

pub(crate) fn export_session(
    providers: &[Box<dyn provider::HistoryProvider>],
    scope: &query_scope::QueryScope,
    format: export::ExportFormat,
    session_id: &str,
    output: Option<&std::path::Path>,
    turn_range: Option<&str>,
    include_notes: bool,
) -> Result<i32, ErrorEnvelope> {
    let discovery = federated_discovery_for_commands(providers, scope);
    let target = lookup_service::load_session_by_selector(
        providers,
        &discovery,
        session_id,
        SelectorShape::SessionRefOrIdPrefix,
    )?;

    let (sliced, turn_offset) = match turn_range {
        Some(spec) => {
            let total = target.messages.len();
            let (start, end) = parse_turn_range(spec, total).map_err(|msg| {
                ErrorEnvelope::new("usage", msg).with_hint(
                    "Use a 1-based range like `12:25`, `:10`, `5:`, or a single turn `7`.",
                )
            })?;
            // start..end are 1-based inclusive bounds; convert to 0-based half-open.
            // The slice's first message is turn `start` in the original session, so
            // we offset turn-keyed notes by `start - 1` to align them.
            (&target.messages[(start - 1)..end], start - 1)
        }
        None => (&target.messages[..], 0usize),
    };

    let notes: Vec<Note> = if include_notes {
        load_session_notes(&target.session_ref, turn_offset, sliced.len())?
    } else {
        Vec::new()
    };

    let content = export::export_with_notes_for_session_ref(
        format,
        &target.session,
        sliced,
        &notes,
        &target.session_ref,
    );

    if let Some(path) = output {
        export::write_file(path, &content).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to write {}: {e}", path.display()),
            )
        })?;
        eprintln!("Exported to {}", path.display());
    } else {
        let mut out = std::io::stdout().lock();
        out.write_all(content.as_bytes())
            .map_err(|e| ErrorEnvelope::io("failed to write export output", e))?;
    }

    Ok(EXIT_OK)
}

#[cfg(test)]
mod tests {
    use super::parse_turn_range;

    #[test]
    fn full_range() {
        assert_eq!(parse_turn_range("3:7", 10).unwrap(), (3, 7));
    }

    #[test]
    fn open_start_defaults_to_one() {
        assert_eq!(parse_turn_range(":5", 10).unwrap(), (1, 5));
    }

    #[test]
    fn open_end_defaults_to_total() {
        assert_eq!(parse_turn_range("4:", 10).unwrap(), (4, 10));
    }

    #[test]
    fn single_turn() {
        assert_eq!(parse_turn_range("7", 10).unwrap(), (7, 7));
    }

    #[test]
    fn end_clamps_to_total() {
        assert_eq!(parse_turn_range("3:999", 10).unwrap(), (3, 10));
    }

    #[test]
    fn empty_session_rejects() {
        assert!(parse_turn_range("1:1", 0).is_err());
    }

    #[test]
    fn zero_rejected() {
        assert!(parse_turn_range("0:5", 10).is_err());
        assert!(parse_turn_range("3:0", 10).is_err());
        assert!(parse_turn_range("0", 10).is_err());
    }

    #[test]
    fn start_past_end_rejects() {
        assert!(parse_turn_range("8:3", 10).is_err());
    }

    #[test]
    fn start_past_session_rejects() {
        assert!(parse_turn_range("99:100", 10).is_err());
    }

    #[test]
    fn non_numeric_rejects() {
        assert!(parse_turn_range("a:b", 10).is_err());
        assert!(parse_turn_range("abc", 10).is_err());
    }

    #[test]
    fn empty_spec_rejects() {
        assert!(parse_turn_range("", 10).is_err());
    }
}
