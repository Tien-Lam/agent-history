use anyhow::Result;

use crate::cli::ShowArgs;
use crate::output;
use crate::provider::registry::Registry;

pub fn run(args: &ShowArgs, registry: &Registry) -> Result<()> {
    let session = registry.find_session(&args.session_id)?;
    let messages = registry.load_messages(&session)?;

    let formatted =
        output::format_messages(&messages, args.format, !args.no_tools, !args.no_thinking);
    print!("{formatted}");
    Ok(())
}
