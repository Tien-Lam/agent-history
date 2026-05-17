use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK, EXIT_USAGE};
use aghist::output::write_json_line;
use aghist::{mcp, schema};

pub(crate) fn schema_command(
    subcommand: Option<&str>,
    list: bool,
    all: bool,
) -> Result<i32, ErrorEnvelope> {
    let payload = if list {
        schema::subcommand_index()
    } else if all {
        schema::all_schemas()
    } else if let Some(name) = subcommand {
        if let Some(value) = schema::schema_for(name) {
            value
        } else {
            let valid = schema::subcommands().join(", ");
            return Err(
                ErrorEnvelope::new("usage", format!("unknown schema subcommand '{name}'"))
                    .with_hint(format!("Valid subcommands: {valid}")),
            );
        }
    } else {
        ErrorEnvelope::new("usage", "schema requires <SUBCMD>, --list, or --all")
            .with_hint("Run `aghist schema --list` to see available subcommands.")
            .emit();
        return Ok(EXIT_USAGE);
    };

    let mut out = io::stdout().lock();
    write_json_line(&mut out, &payload)
        .map_err(|e| ErrorEnvelope::io("failed to write schema output", e))?;
    Ok(EXIT_OK)
}

pub(crate) fn run_mcp_server(server: &mcp::McpServer) -> Result<i32, ErrorEnvelope> {
    let stdin = io::stdin().lock();
    let stdout = io::stdout().lock();
    server
        .serve(stdin, stdout)
        .map_err(|e| ErrorEnvelope::io("MCP server stdio error", e))?;
    Ok(EXIT_OK)
}
