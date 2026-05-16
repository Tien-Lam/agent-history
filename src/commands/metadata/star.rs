use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::metadata::{self, Star};
use aghist::output::OutputMode;

use super::{metadata_error, open_metadata_db};

pub(crate) fn star_command(reference: &str, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    let row = metadata::star_add(&conn, reference).map_err(|e| metadata_error(&e))?;
    emit_star_payload(&row, "starred", mode)?;
    Ok(EXIT_OK)
}

pub(crate) fn unstar_command(reference: &str, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    let row = metadata::star_remove(&conn, reference).map_err(|e| metadata_error(&e))?;
    emit_star_payload(&row, "unstarred", mode)?;
    Ok(EXIT_OK)
}

pub(crate) fn stars_list(reference: Option<&str>, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    let stars = metadata::star_list(&conn, reference).map_err(|e| metadata_error(&e))?;
    emit_star_list(&stars, mode)?;
    if stars.is_empty() {
        Ok(EXIT_EMPTY)
    } else {
        Ok(EXIT_OK)
    }
}

fn emit_star_payload(star: &Star, action: &str, mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if mode.is_machine() {
        let payload = serde_json::json!({ action: star });
        serde_json::to_writer(&mut out, &payload)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
        writeln!(out).ok();
    } else {
        writeln!(out, "{action} {}", star.session_ref).ok();
    }
    Ok(())
}

fn emit_star_list(stars: &[Star], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "stars": stars, "count": stars.len() });
            serde_json::to_writer(&mut out, &payload)
                .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
            writeln!(out).ok();
        }
        OutputMode::Ndjson => {
            for star in stars {
                serde_json::to_writer(&mut out, star).map_err(|e| {
                    ErrorEnvelope::new("io-error", format!("failed to emit NDJSON row: {e}"))
                })?;
                writeln!(out).ok();
            }
        }
        OutputMode::Human => {
            if stars.is_empty() {
                writeln!(out, "(no stars)").ok();
            } else {
                for star in stars {
                    writeln!(out, "* {} (starred {})", star.session_ref, star.starred_at).ok();
                }
            }
        }
    }
    Ok(())
}
