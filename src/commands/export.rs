use std::io::Write as _;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK};
use aghist::services::lookup as lookup_service;
use aghist::session_resolver::SelectorShape;
use aghist::{export, provider, query_scope};

use super::discovery::federated_discovery_for_commands;
use notes::load_session_notes;
use range::parse_turn_range;

mod notes;
mod range;

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
            let start_idx = start - 1;
            let Some(slice) = target.messages.get(start_idx..end) else {
                return Err(ErrorEnvelope::new(
                    "usage",
                    format!(
                        "turn range '{spec}' is outside session bounds ({} message(s))",
                        target.messages.len()
                    ),
                ));
            };
            (slice, start_idx)
        }
        None => (&target.messages[..], 0usize),
    };

    let notes = if include_notes {
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
