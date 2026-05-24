use aghist::model::Provider;
use clap::Args;

use crate::cli::resolvers::parse_provider_slug;

#[derive(Args)]
pub(crate) struct IndexCommand {
    /// Reindex only sessions from this provider
    /// (`claude-code`, `copilot-cli`, `gemini-cli`, `codex-cli`, `opencode`,
    /// `cursor`, `aider`, `zed-ai`, `cline`, `continue-dev`).
    #[arg(long, value_parser = parse_provider_slug, conflicts_with = "params")]
    pub(crate) provider: Option<Provider>,

    /// Force a full rebuild by clearing the index first.
    #[arg(long, conflicts_with = "params")]
    pub(crate) force: bool,

    /// Authorise the one-off download of the embedding model
    /// (~90 MB `AllMiniLML6V2`). Required the first time semantic indexing
    /// runs; consent is persisted next to the index, so subsequent runs
    /// don't need this flag. Without consent (and without this flag),
    /// indexing stays purely lexical.
    #[arg(long, conflicts_with = "params")]
    pub(crate) accept_download: bool,

    /// JSON request body containing all params at once. Mutually exclusive
    /// with other flags. Schema: `{provider?, force?, accept_download?}`.
    #[arg(long, value_name = "JSON")]
    pub(crate) params: Option<String>,
}
