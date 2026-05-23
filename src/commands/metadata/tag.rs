use aghist::cli_error::{ErrorEnvelope, EXIT_EMPTY, EXIT_OK};
use aghist::metadata;
use aghist::output::OutputMode;

use super::super::super::cli::TagCommand;
use super::{metadata_error, open_metadata_db};

mod output;

use output::{emit_tag_list, emit_tag_payload};

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
