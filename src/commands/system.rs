use std::io;

use aghist::cli_error::{ErrorEnvelope, EXIT_OK, EXIT_USAGE};
use aghist::{mcp, provider, schema};

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

    serde_json::to_writer(io::stdout().lock(), &payload).map_err(|e| {
        ErrorEnvelope::new("io-error", format!("failed to write schema output: {e}"))
    })?;
    println!();
    Ok(EXIT_OK)
}

pub(crate) fn run_mcp(
    providers: Vec<Box<dyn provider::HistoryProvider>>,
) -> Result<i32, ErrorEnvelope> {
    let stdin = io::stdin().lock();
    let stdout = io::stdout().lock();
    let server = mcp::McpServer::new(providers);
    server
        .serve(stdin, stdout)
        .map_err(|e| ErrorEnvelope::new("io-error", format!("MCP server stdio error: {e}")))?;
    Ok(EXIT_OK)
}
