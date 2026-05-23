use std::collections::HashSet;
use std::hash::BuildHasher;

use crate::cli_error::ErrorEnvelope;
use crate::model::Provider;

pub(super) fn ensure_provider_visible<S: BuildHasher>(
    provider: Option<Provider>,
    visible_providers: Option<&HashSet<Provider, S>>,
) -> Result<(), ErrorEnvelope> {
    let Some(provider) = provider else {
        return Ok(());
    };
    if visible_providers.is_none_or(|visible| visible.contains(&provider)) {
        Ok(())
    } else {
        Err(ErrorEnvelope::new(
            "provider-unavailable",
            format!(
                "provider '{}' is not enabled or not visible to MCP",
                provider.slug()
            ),
        ))
    }
}
