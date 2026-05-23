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
        session_id: after_provider.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn provider_strategy() -> impl Strategy<Value = Provider> {
        prop::sample::select(Provider::all().to_vec())
    }

    fn source_name_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just(LOCAL_SOURCE.to_string()),
            "[A-Za-z0-9][A-Za-z0-9_-]{0,24}"
                .prop_filter("source name must not be reserved", |name| name != "local"),
        ]
    }

    fn session_id_strategy() -> impl Strategy<Value = String> {
        "[A-Za-z0-9][A-Za-z0-9_.:-]{0,80}"
    }

    proptest! {
        #[test]
        fn session_uris_roundtrip_generated_parts(
            source in source_name_strategy(),
            provider in provider_strategy(),
            session_id in session_id_strategy(),
        ) {
            let uri = session_uri_for_source(&source, provider, &session_id);

            let parsed = parse_aghist_uri(&uri).unwrap();

            match parsed {
                ParsedUri::Session {
                    source: parsed_source,
                    provider: parsed_provider,
                    session_id: parsed_session_id,
                } => {
                    let expected_source = (source != LOCAL_SOURCE).then_some(source);
                    prop_assert_eq!(parsed_source, expected_source);
                    prop_assert_eq!(parsed_provider, provider);
                    prop_assert_eq!(parsed_session_id, session_id);
                }
                ParsedUri::Turn { .. } => prop_assert!(false, "session URI parsed as turn URI"),
            }
        }

        #[test]
        fn turn_uris_roundtrip_generated_parts(
            source in source_name_strategy(),
            provider in provider_strategy(),
            session_id in session_id_strategy(),
            turn in 1u32..=u32::MAX,
        ) {
            let uri = turn_uri_for_source(&source, provider, &session_id, turn);

            let parsed = parse_aghist_uri(&uri).unwrap();

            match parsed {
                ParsedUri::Turn {
                    source: parsed_source,
                    provider: parsed_provider,
                    session_id: parsed_session_id,
                    turn: parsed_turn,
                } => {
                    let expected_source = (source != LOCAL_SOURCE).then_some(source);
                    prop_assert_eq!(parsed_source, expected_source);
                    prop_assert_eq!(parsed_provider, provider);
                    prop_assert_eq!(parsed_session_id, session_id);
                    prop_assert_eq!(parsed_turn, turn);
                }
                ParsedUri::Session { .. } => prop_assert!(false, "turn URI parsed as session URI"),
            }
        }
    }
}
