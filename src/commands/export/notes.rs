use aghist::cli_error::ErrorEnvelope;
use aghist::metadata::{self as metadata_store, Note};

use crate::commands::metadata::metadata_error;

/// Pull the session's notes from the metadata sidecar, restricted to the
/// exported turn range. Missing sidecar is a no-op; an existing but unreadable
/// or corrupt sidecar is an error because `--include-notes` was explicit.
/// Notes that fall outside the slice are dropped; turn-level notes have their
/// `session_ref` rebased so turn N in the full session becomes turn
/// `N - turn_offset` in the slice.
pub(super) fn load_session_notes(
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
