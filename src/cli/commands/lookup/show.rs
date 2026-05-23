use aghist::schema_fragments::SHOW_INCLUDE_CONTEXT_DEFAULT;
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
    #[arg(long, default_value_t = SHOW_INCLUDE_CONTEXT_DEFAULT, conflicts_with = "params")]
    pub(crate) include_context: u32,

    /// JSON request body containing all params at once. Mutually exclusive
    /// with other flags. Schema: `{reference, format?, include_context?}`.
    #[arg(long, value_name = "JSON")]
    pub(crate) params: Option<String>,
}
