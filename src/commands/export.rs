use std::io::Write as _;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::metadata::{self, Note};
use aghist::model::Session;
use aghist::{export, provider};

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

/// Best-effort: pull the session's notes from the metadata sidecar, restricted
/// to the exported turn range. Returns `None` if the sidecar can't be opened
/// (the common case: the user hasn't created one yet). Notes that fall outside
/// the slice are dropped; turn-level notes have their `session_ref` rebased so
/// turn N in the full session becomes turn `N - turn_offset` in the slice.
fn load_session_notes(
    session: &Session,
    turn_offset: usize,
    slice_len: usize,
) -> Option<Vec<Note>> {
    let path = metadata::default_path()?;
    if !path.exists() {
        return None;
    }
    let conn = metadata::open(&path).ok()?;
    let session_ref = session.session_ref().to_string();
    let all = metadata::note_list(&conn, Some(&session_ref)).ok()?;
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
    Some(out)
}

pub(crate) fn export_session(
    providers: &[Box<dyn provider::HistoryProvider>],
    format: export::ExportFormat,
    session_id: &str,
    output: Option<&std::path::Path>,
    turn_range: Option<&str>,
    include_notes: bool,
) -> Result<i32, ErrorEnvelope> {
    let mut all_sessions = Vec::new();
    for p in providers {
        if let Ok(sessions) = p.discover_sessions() {
            all_sessions.extend(sessions);
        }
    }

    let session = all_sessions
        .iter()
        .find(|s| s.id.0 == session_id || s.id.0.starts_with(session_id))
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "session-not-found",
                format!("Session not found: {session_id}"),
            )
            .with_hint("Run `aghist --list` to see available session IDs.")
        })?;

    let provider = providers
        .iter()
        .find(|p| p.provider() == session.provider)
        .ok_or_else(|| {
            ErrorEnvelope::new(
                "provider-unavailable",
                format!(
                    "Provider {} is not enabled for session {}",
                    session.provider, session.id.0
                ),
            )
            .with_hint("Enable the provider in your config (`providers` table).")
        })?;

    let messages = provider.load_messages(session).map_err(|e| {
        ErrorEnvelope::new(
            "provider-error",
            format!("failed to load messages for {}: {e}", session.id.0),
        )
    })?;

    let (sliced, turn_offset) = match turn_range {
        Some(spec) => {
            let total = messages.len();
            let (start, end) = parse_turn_range(spec, total).map_err(|msg| {
                ErrorEnvelope::new("usage", msg).with_hint(
                    "Use a 1-based range like `12:25`, `:10`, `5:`, or a single turn `7`.",
                )
            })?;
            // start..end are 1-based inclusive bounds; convert to 0-based half-open.
            // The slice's first message is turn `start` in the original session, so
            // we offset turn-keyed notes by `start - 1` to align them.
            (&messages[(start - 1)..end], start - 1)
        }
        None => (&messages[..], 0usize),
    };

    let notes: Vec<Note> = if include_notes {
        load_session_notes(session, turn_offset, sliced.len()).unwrap_or_default()
    } else {
        Vec::new()
    };

    let content = export::export_with_notes(format, session, sliced, &notes);

    if let Some(path) = output {
        std::fs::write(path, &content).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to write {}: {e}", path.display()),
            )
        })?;
        eprintln!("Exported to {}", path.display());
    } else {
        let mut out = std::io::stdout().lock();
        out.write_all(content.as_bytes()).map_err(|e| {
            ErrorEnvelope::new("io-error", format!("failed to write export output: {e}"))
        })?;
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
