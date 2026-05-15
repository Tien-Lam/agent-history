use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::metadata::{self, MetadataError, Note, Star, Tag};
use aghist::model::Provider;
use aghist::output::OutputMode;
use aghist::search;

use super::super::cli::{NoteCommand, TagCommand};

pub(crate) fn note_dispatch(command: NoteCommand, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    match command {
        NoteCommand::Add {
            reference,
            body,
            body_file,
            stdin,
        } => {
            let body = read_note_body(body.as_deref(), body_file.as_deref(), stdin)?;
            let note =
                metadata::note_add(&conn, &reference, &body).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "added", mode)?;
            Ok(EXIT_OK)
        }
        NoteCommand::List { reference, json } => {
            let mode = if json { OutputMode::Json } else { mode };
            let notes =
                metadata::note_list(&conn, reference.as_deref()).map_err(|e| metadata_error(&e))?;
            emit_note_list(&notes, mode)?;
            if notes.is_empty() {
                Ok(EXIT_EMPTY)
            } else {
                Ok(EXIT_OK)
            }
        }
        NoteCommand::Edit {
            id,
            body,
            body_file,
            stdin,
        } => {
            let body = read_note_body(body.as_deref(), body_file.as_deref(), stdin)?;
            let note = metadata::note_edit(&conn, id, &body).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "updated", mode)?;
            Ok(EXIT_OK)
        }
        NoteCommand::Remove { id } => {
            let note = metadata::note_remove(&conn, id).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "removed", mode)?;
            Ok(EXIT_OK)
        }
    }
}

pub(crate) fn open_metadata_db() -> Result<rusqlite::Connection, ErrorEnvelope> {
    metadata::open_default().map_err(|e| metadata_error(&e))
}

/// Best-effort: open the metadata sidecar and feed every note into the search
/// index. Any failure is swallowed; metadata is optional and search must keep
/// working without it.
pub(crate) fn try_index_notes(index: &search::SearchIndex) {
    let Some(path) = metadata::default_path() else {
        return;
    };
    if !path.exists() {
        return;
    }
    let Ok(conn) = metadata::open(&path) else {
        return;
    };
    let Ok(notes) = metadata::note_list(&conn, None) else {
        return;
    };
    let _ = index.index_notes(&notes);
}

pub(crate) fn metadata_error(err: &MetadataError) -> ErrorEnvelope {
    match err {
        MetadataError::NoPath => {
            ErrorEnvelope::new("config-error", "could not resolve metadata.db path").with_hint(
                "Set AGHIST_METADATA_DB=/path/to/metadata.db, or ensure XDG/home dirs exist.",
            )
        }
        MetadataError::CreateDir { ref path, .. } => ErrorEnvelope::new(
            "io-error",
            format!("could not create metadata dir {}: {err}", path.display()),
        ),
        MetadataError::Open { ref path, .. } => ErrorEnvelope::new(
            "io-error",
            format!("could not open metadata.db at {}: {err}", path.display()),
        ),
        MetadataError::Migrate { ref path, .. } => ErrorEnvelope::new(
            "metadata-error",
            format!("metadata.db migration failed at {}: {err}", path.display()),
        ),
        MetadataError::InvalidSessionRef(_, _) => {
            let valid = Provider::all()
                .iter()
                .map(|provider| provider.slug())
                .collect::<Vec<_>>()
                .join(", ");
            ErrorEnvelope::new("invalid-ref", err.to_string()).with_hint(format!(
                "Use '<provider>/<session-id>' or '<provider>/<session-id>#<turn>'. Valid providers: {valid}."
            ))
        }
        MetadataError::EmptyBody => ErrorEnvelope::new("usage", "note body must not be empty")
            .with_hint("Pass --body \"text\", --body-file PATH, or --stdin."),
        MetadataError::NoteNotFound(id) => {
            ErrorEnvelope::new("note-not-found", format!("no note with id {id}"))
                .with_hint("Run `aghist note list` to see existing note ids.")
        }
        MetadataError::EmptyTag => ErrorEnvelope::new("usage", "tag must not be empty")
            .with_hint("Pass a non-empty tag value, e.g. `aghist tag add <ref> review`."),
        MetadataError::TagAlreadyExists { session_ref, tag } => ErrorEnvelope::new(
            "tag-conflict",
            format!("tag '{tag}' is already attached to {session_ref}"),
        )
        .with_hint("Each (session_ref, tag) pair is unique. Use a different tag, or remove the existing one first."),
        MetadataError::TagNotFound { session_ref, tag } => ErrorEnvelope::new(
            "tag-not-found",
            format!("tag '{tag}' is not attached to {session_ref}"),
        )
        .with_hint("Run `aghist tag list <ref>` to see attached tags."),
        MetadataError::StarAlreadyExists { session_ref } => ErrorEnvelope::new(
            "star-conflict",
            format!("{session_ref} is already starred"),
        )
        .with_hint("Each session ref can be starred at most once. Use `aghist unstar <ref>` first if you want to re-star."),
        MetadataError::StarNotFound { session_ref } => ErrorEnvelope::new(
            "star-not-found",
            format!("{session_ref} is not starred"),
        )
        .with_hint("Run `aghist stars` to see starred refs."),
        MetadataError::Sqlite(_) => ErrorEnvelope::new("metadata-error", err.to_string()),
    }
}

fn read_note_body(
    body: Option<&str>,
    body_file: Option<&std::path::Path>,
    stdin: bool,
) -> Result<String, ErrorEnvelope> {
    use std::io::Read;
    if let Some(b) = body {
        return Ok(b.to_string());
    }
    let mut buf = String::new();
    if stdin {
        io::stdin().read_to_string(&mut buf).map_err(|e| {
            ErrorEnvelope::new(
                "io-error",
                format!("failed to read note body from stdin: {e}"),
            )
        })?;
        return Ok(buf);
    }
    if let Some(path) = body_file {
        if path == std::path::Path::new("-") {
            io::stdin().read_to_string(&mut buf).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to read note body from stdin: {e}"),
                )
            })?;
        } else {
            buf = std::fs::read_to_string(path).map_err(|e| {
                ErrorEnvelope::new(
                    "io-error",
                    format!("failed to read note body from {}: {e}", path.display()),
                )
            })?;
        }
        return Ok(buf);
    }
    Err(ErrorEnvelope::new(
        "usage",
        "note body required: pass --body, --body-file, or --stdin",
    ))
}

fn emit_note_payload(note: &Note, action: &str, mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if mode.is_machine() {
        let payload = serde_json::json!({ action: note });
        serde_json::to_writer(&mut out, &payload)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
        writeln!(out).ok();
    } else {
        writeln!(out, "{action} note {} on {}", note.id, note.session_ref).ok();
        for line in note.body.lines() {
            writeln!(out, "  {line}").ok();
        }
    }
    Ok(())
}

fn emit_note_list(notes: &[Note], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "notes": notes, "count": notes.len() });
            serde_json::to_writer(&mut out, &payload)
                .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
            writeln!(out).ok();
        }
        OutputMode::Ndjson => {
            for note in notes {
                serde_json::to_writer(&mut out, note).map_err(|e| {
                    ErrorEnvelope::new("io-error", format!("failed to emit NDJSON row: {e}"))
                })?;
                writeln!(out).ok();
            }
        }
        OutputMode::Human => {
            if notes.is_empty() {
                writeln!(out, "(no notes)").ok();
            } else {
                for note in notes {
                    writeln!(
                        out,
                        "#{} {} (created {}, updated {})",
                        note.id, note.session_ref, note.created_at, note.updated_at
                    )
                    .ok();
                    for line in note.body.lines() {
                        writeln!(out, "  {line}").ok();
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn tag_dispatch(command: TagCommand, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    let conn = open_metadata_db()?;
    match command {
        TagCommand::Add { reference, tag } => {
            let row = metadata::tag_add(&conn, &reference, &tag).map_err(|e| metadata_error(&e))?;
            emit_tag_payload(&row, "added", mode)?;
            Ok(EXIT_OK)
        }
        TagCommand::List {
            reference,
            tag,
            json,
        } => {
            let mode = if json { OutputMode::Json } else { mode };
            let tags = metadata::tag_list(&conn, reference.as_deref(), tag.as_deref())
                .map_err(|e| metadata_error(&e))?;
            emit_tag_list(&tags, mode)?;
            if tags.is_empty() {
                Ok(EXIT_EMPTY)
            } else {
                Ok(EXIT_OK)
            }
        }
        TagCommand::Remove { reference, tag } => {
            let row =
                metadata::tag_remove(&conn, &reference, &tag).map_err(|e| metadata_error(&e))?;
            emit_tag_payload(&row, "removed", mode)?;
            Ok(EXIT_OK)
        }
    }
}

fn emit_tag_payload(tag: &Tag, action: &str, mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if mode.is_machine() {
        let payload = serde_json::json!({ action: tag });
        serde_json::to_writer(&mut out, &payload)
            .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
        writeln!(out).ok();
    } else {
        writeln!(out, "{action} tag '{}' on {}", tag.tag, tag.session_ref).ok();
    }
    Ok(())
}

fn emit_tag_list(tags: &[Tag], mode: OutputMode) -> Result<(), ErrorEnvelope> {
    use std::io::Write as _;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match mode {
        OutputMode::Json => {
            let payload = serde_json::json!({ "tags": tags, "count": tags.len() });
            serde_json::to_writer(&mut out, &payload)
                .map_err(|e| ErrorEnvelope::new("io-error", format!("failed to emit JSON: {e}")))?;
            writeln!(out).ok();
        }
        OutputMode::Ndjson => {
            for tag in tags {
                serde_json::to_writer(&mut out, tag).map_err(|e| {
                    ErrorEnvelope::new("io-error", format!("failed to emit NDJSON row: {e}"))
                })?;
                writeln!(out).ok();
            }
        }
        OutputMode::Human => {
            if tags.is_empty() {
                writeln!(out, "(no tags)").ok();
            } else {
                for tag in tags {
                    writeln!(
                        out,
                        "#{} {} [{}] (created {})",
                        tag.id, tag.session_ref, tag.tag, tag.created_at
                    )
                    .ok();
                }
            }
        }
    }
    Ok(())
}

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
