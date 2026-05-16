//! Self-describing JSON-Schema (draft-2020-12) for `aghist` subcommands.
//!
//! Agents (and humans) can run `aghist schema <subcmd>` to get a stable
//! machine-readable description of a subcommand's flags, response shape, and
//! exit codes — instead of scraping `--help` or relying on documentation that
//! drifts away from the binary.
//!
//! The schemas are hand-authored rather than derived: the CLI surface is small
//! and stable, agents need exit-code semantics and response shapes that clap's
//! introspection doesn't provide, and the schema doubles as the contract we
//! commit to.

mod analysis;
mod common;
mod core;
mod insights;
mod metadata;

#[cfg(test)]
mod tests;

use serde_json::{json, Value};

use crate::command_spec::{command_names, command_spec};

/// Command targets that expose a schema, in stable discovery order.
pub fn subcommands() -> Vec<&'static str> {
    command_names().collect()
}

/// Return the schema for a subcommand, or `None` if the name is unknown.
pub fn schema_for(subcmd: &str) -> Option<Value> {
    command_spec(subcmd)?;
    match subcmd {
        "list" => Some(core::list_schema()),
        "search" => Some(core::search_schema()),
        "show" => Some(core::show_schema()),
        "export" => Some(core::export_schema()),
        "diff" => Some(core::diff_schema()),
        "index" => Some(core::index_schema()),
        "sources" => Some(core::sources_schema()),
        "health" => Some(core::health_schema()),
        "mcp" => Some(core::mcp_schema()),
        "schema" => Some(core::schema_schema()),
        "decisions" => Some(analysis::decisions_schema()),
        "todos" => Some(analysis::todos_schema()),
        "threads" => Some(analysis::threads_schema()),
        "track" => Some(analysis::track_schema()),
        "note" => Some(metadata::note_schema()),
        "tag" => Some(metadata::tag_schema()),
        "star" => Some(metadata::star_schema()),
        "unstar" => Some(metadata::unstar_schema()),
        "stars" => Some(metadata::stars_schema()),
        "usage" => Some(insights::usage_schema()),
        "project" => Some(insights::project_schema()),
        "report" => Some(insights::report_schema()),
        _ => None,
    }
}

/// Return a JSON object containing the schema for every known subcommand,
/// keyed by name. Useful for one-shot discovery.
pub fn all_schemas() -> Value {
    let mut map = serde_json::Map::new();
    for name in subcommands() {
        if let Some(schema) = schema_for(name) {
            map.insert(name.to_string(), schema);
        }
    }
    Value::Object(map)
}

/// JSON list of subcommand names, suitable for `aghist schema --list`.
pub fn subcommand_index() -> Value {
    json!({
        "subcommands": subcommands(),
    })
}
