use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::metadata;
use aghist::output::OutputMode;
use aghist::schema_fragments::METADATA_NOTE_BODY_MAX_BYTES;

use super::super::super::cli::NoteCommand;
use super::super::context::output_mode_with_local_json;
use super::super::input::{read_text_input_with_limit, TextInput, TextInputMessages};
use super::{metadata_error, open_metadata_db};

mod output;

use output::{emit_note_list, emit_note_payload};

pub(crate) fn note_dispatch(command: NoteCommand, mode: OutputMode) -> Result<i32, ErrorEnvelope> {
    match command {
        NoteCommand::Add {
            reference,
            body,
            body_file,
            stdin,
        } => {
            let body = read_note_body(body.as_deref(), body_file.as_deref(), stdin)?;
            let conn = open_metadata_db()?;
            let note =
                metadata::note_add(&conn, &reference, &body).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "added", mode)?;
            Ok(EXIT_OK)
        }
        NoteCommand::List { reference, json } => {
            let mode = output_mode_with_local_json(mode, json)?;
            let conn = open_metadata_db()?;
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
            let conn = open_metadata_db()?;
            let note = metadata::note_edit(&conn, id, &body).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "updated", mode)?;
            Ok(EXIT_OK)
        }
        NoteCommand::Remove { id } => {
            let conn = open_metadata_db()?;
            let note = metadata::note_remove(&conn, id).map_err(|e| metadata_error(&e))?;
            emit_note_payload(&note, "removed", mode)?;
            Ok(EXIT_OK)
        }
    }
}

fn read_note_body(
    body: Option<&str>,
    body_file: Option<&std::path::Path>,
    stdin: bool,
) -> Result<String, ErrorEnvelope> {
    read_text_input_with_limit(
        TextInput {
            inline: body,
            file: body_file,
            stdin,
        },
        TextInputMessages {
            missing: "note body required: pass --body, --body-file, or --stdin",
            multiple: "note body accepts only one input source",
            stdin_read: "failed to read note body from stdin",
            file_read_prefix: "failed to read note body from",
            usage_hint: None,
        },
        false,
        METADATA_NOTE_BODY_MAX_BYTES,
        "note body",
    )
}
