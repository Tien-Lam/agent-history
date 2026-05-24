use aghist::schema_fragments::{SHOW_INCLUDE_CONTEXT_DEFAULT, SHOW_INCLUDE_CONTEXT_MAX};
use clap::Args;

use crate::cli::resolvers::ShowFormat;

#[derive(Args)]
pub(crate) struct ShowCommand {
    /// Citation ref. E.g. `claude-code/abc-123#7`.
    #[arg(
        value_name = "REF",
        conflicts_with = "params",
        required_unless_present = "params"
    )]
    pub(crate) reference: Option<String>,

    /// Output format: md (default), json, text.
    #[arg(long, short, default_value = "md", conflicts_with = "params")]
    pub(crate) format: ShowFormat,

    /// Include N turns before and after the target for context (default 0).
    #[arg(long, default_value_t = SHOW_INCLUDE_CONTEXT_DEFAULT, value_parser = parse_include_context, conflicts_with = "params")]
    pub(crate) include_context: u32,

    /// JSON request body containing all params at once. Mutually exclusive
    /// with other flags. Schema: `{reference, format?, include_context?}`.
    #[arg(long, value_name = "JSON")]
    pub(crate) params: Option<String>,
}

fn parse_include_context(raw: &str) -> Result<u32, String> {
    let value = raw
        .parse::<u32>()
        .map_err(|e| format!("invalid include context: {e}"))?;
    if value > SHOW_INCLUDE_CONTEXT_MAX {
        Err(format!(
            "include context must be at most {SHOW_INCLUDE_CONTEXT_MAX}"
        ))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_include_context_rejects_values_above_max() {
        assert_eq!(
            parse_include_context(&(SHOW_INCLUDE_CONTEXT_MAX + 1).to_string()).unwrap_err(),
            format!("include context must be at most {SHOW_INCLUDE_CONTEXT_MAX}")
        );
    }
}
