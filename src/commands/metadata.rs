use aghist::cli_error::ErrorEnvelope;
use aghist::metadata::{self, MetadataError};
use aghist::model::Provider;
mod note;
mod star;
mod tag;

pub(crate) use note::note_dispatch;
pub(crate) use star::{star_command, stars_list, unstar_command};
pub(crate) use tag::tag_dispatch;

pub(crate) fn open_metadata_db() -> Result<rusqlite::Connection, ErrorEnvelope> {
    metadata::open_default().map_err(|e| metadata_error(&e))
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
        MetadataError::SessionRefTooLong { max_bytes, .. } => ErrorEnvelope::new(
            "usage",
            format!("session ref must be at most {max_bytes} bytes"),
        )
        .with_hint("Use a shorter session id, session ref, or citation ref."),
        MetadataError::EmptyBody => ErrorEnvelope::new("usage", "note body must not be empty")
            .with_hint("Pass --body \"text\", --body-file PATH, or --stdin."),
        MetadataError::BodyTooLong { max_bytes, .. } => ErrorEnvelope::new(
            "usage",
            format!("note body must be at most {max_bytes} bytes"),
        )
        .with_hint("Shorten the note or store long-form context outside the metadata sidecar."),
        MetadataError::NoteFilterTooLong { max_bytes, .. } => ErrorEnvelope::new(
            "usage",
            format!("note filter must be at most {max_bytes} bytes"),
        )
        .with_hint("Use a shorter --note substring."),
        MetadataError::NoteNotFound(id) => {
            ErrorEnvelope::new("note-not-found", format!("no note with id {id}"))
                .with_hint("Run `aghist note list` to see existing note ids.")
        }
        MetadataError::EmptyTag => ErrorEnvelope::new("usage", "tag must not be empty")
            .with_hint("Pass a non-empty tag value, e.g. `aghist tag add <ref> review`."),
        MetadataError::TagTooLong { max_bytes, .. } => {
            ErrorEnvelope::new("usage", format!("tag must be at most {max_bytes} bytes"))
                .with_hint("Use a shorter tag label.")
        }
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
