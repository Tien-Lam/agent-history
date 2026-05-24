use crate::federated::LOCAL_SOURCE;
use crate::model::Provider;

const URI_PREFIX: &str = "aghist://session/";
const SOURCE_URI_PREFIX: &str = "aghist://source/";

pub(crate) fn session_uri(provider: Provider, session_id: &str) -> String {
    format!("{URI_PREFIX}{}/{session_id}", provider.slug())
}

pub(crate) fn session_uri_for_source(source: &str, provider: Provider, session_id: &str) -> String {
    if source == LOCAL_SOURCE {
        session_uri(provider, session_id)
    } else {
        format!(
            "{SOURCE_URI_PREFIX}{source}/session/{}/{session_id}",
            provider.slug()
        )
    }
}

pub(crate) fn turn_uri(provider: Provider, session_id: &str, turn: u32) -> String {
    format!("{URI_PREFIX}{}/{session_id}/turn/{turn}", provider.slug())
}

pub(crate) fn turn_uri_for_source(
    source: &str,
    provider: Provider,
    session_id: &str,
    turn: u32,
) -> String {
    if source == LOCAL_SOURCE {
        turn_uri(provider, session_id, turn)
    } else {
        format!(
            "{SOURCE_URI_PREFIX}{source}/session/{}/{session_id}/turn/{turn}",
            provider.slug()
        )
    }
}

pub(crate) enum ParsedUri {
    Session {
        source: Option<String>,
        provider: Provider,
        session_id: String,
    },
    Turn {
        source: Option<String>,
        provider: Provider,
        session_id: String,
        turn: u32,
    },
}

/// Parses `aghist://session/<provider>/<session-id>[/turn/<n>]`.
///
/// Session IDs are taken verbatim - the same convention citation refs use -
/// so anything past the provider segment up to an optional `/turn/<n>` tail
/// is the session id. We don't URL-decode: provider slugs are kebab-case
/// ASCII, and every session id we discover today is filesystem-safe.
pub(crate) fn parse_aghist_uri(uri: &str) -> Result<ParsedUri, String> {
    if let Some(rest) = uri.strip_prefix(SOURCE_URI_PREFIX) {
        let (source, after_source) = rest
            .split_once("/session/")
            .ok_or_else(|| "missing '/session/' segment after source".to_string())?;
        crate::config::validate_source_name(source)?;
        if after_source.is_empty() {
            return Err("missing provider segment".to_string());
        }
        return parse_provider_session_tail(Some(source.to_string()), after_source);
    }

    let rest = uri
        .strip_prefix(URI_PREFIX)
        .ok_or_else(|| format!("uri must start with '{URI_PREFIX}'"))?;
    parse_provider_session_tail(None, rest)
}

fn parse_provider_session_tail(source: Option<String>, rest: &str) -> Result<ParsedUri, String> {
    if rest.is_empty() {
        return Err("missing provider segment".to_string());
    }

    let (provider_slug, after_provider) = rest
        .split_once('/')
        .ok_or_else(|| "missing session id".to_string())?;
    if provider_slug.is_empty() {
        return Err("missing provider segment".to_string());
    }
    let provider = Provider::from_slug(provider_slug)
        .ok_or_else(|| format!("unknown provider slug '{provider_slug}'"))?;
    if after_provider.is_empty() {
        return Err("missing session id".to_string());
    }

    if let Some((session_id, turn_str)) = after_provider.rsplit_once("/turn/") {
        if session_id.is_empty() {
            return Err("missing session id".to_string());
        }
        validate_session_id(session_id)?;
        if turn_str.is_empty() {
            return Err("missing turn number".to_string());
        }
        let turn: u32 = turn_str
            .parse()
            .map_err(|_| format!("invalid turn '{turn_str}' (must be a positive integer)"))?;
        if turn == 0 {
            return Err("turn must be >= 1".to_string());
        }
        return Ok(ParsedUri::Turn {
            source,
            provider,
            session_id: session_id.to_string(),
            turn,
        });
    }

    Ok(ParsedUri::Session {
        source,
        provider,
        session_id: validate_session_id(after_provider)?.to_string(),
    })
}

fn validate_session_id(session_id: &str) -> Result<&str, String> {
    if session_id.chars().any(char::is_control) {
        return Err("session id must not contain control characters".to_string());
    }
    Ok(session_id)
}

#[cfg(test)]
mod tests;
