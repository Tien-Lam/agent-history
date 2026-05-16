use serde_json::{json, Value};

use crate::model::{Provider, Session};

const URI_PREFIX: &str = "aghist://session/";

pub(super) fn session_uri(provider: Provider, session_id: &str) -> String {
    format!("{URI_PREFIX}{}/{session_id}", provider.slug())
}

pub(super) fn turn_uri(provider: Provider, session_id: &str, turn: u32) -> String {
    format!("{URI_PREFIX}{}/{session_id}/turn/{turn}", provider.slug())
}

pub(super) fn resource_descriptor(s: &Session) -> Value {
    let title = s
        .summary
        .clone()
        .or_else(|| s.project_name.clone())
        .unwrap_or_else(|| s.id.0.clone());
    let description = format!(
        "{} session ({} messages){}",
        s.provider.as_str(),
        s.message_count,
        s.project_name
            .as_deref()
            .map(|p| format!(" — {p}"))
            .unwrap_or_default(),
    );
    json!({
        "uri": session_uri(s.provider, &s.id.0),
        "name": title,
        "description": description,
        "mimeType": "application/json",
    })
}

pub(super) fn resource_templates() -> Value {
    let provider_slugs = Provider::all()
        .iter()
        .map(|provider| provider.slug())
        .collect::<Vec<_>>()
        .join(", ");
    let session_description = format!(
        "Full session metadata + ordered turns. `provider` is the kebab-case slug ({provider_slugs})."
    );
    json!([
        {
            "uriTemplate": "aghist://session/{provider}/{session_id}",
            "name": "Session",
            "description": session_description,
            "mimeType": "application/json"
        },
        {
            "uriTemplate": "aghist://session/{provider}/{session_id}/turn/{turn}",
            "name": "Session turn",
            "description": "A single 1-based turn within a session. \
                            The triple `(provider, session_id, turn)` matches \
                            the citation-ref format.",
            "mimeType": "application/json"
        }
    ])
}

pub(super) enum ParsedUri {
    Session {
        provider: Provider,
        session_id: String,
    },
    Turn {
        provider: Provider,
        session_id: String,
        turn: u32,
    },
}

/// Parses `aghist://session/<provider>/<session-id>[/turn/<n>]`.
///
/// Session IDs are taken verbatim — the same convention citation refs use —
/// so anything past the provider segment up to an optional `/turn/<n>` tail
/// is the session id. We don't URL-decode: provider slugs are kebab-case
/// ASCII, and every session id we discover today is filesystem-safe.
pub(super) fn parse_aghist_uri(uri: &str) -> Result<ParsedUri, String> {
    let rest = uri
        .strip_prefix(URI_PREFIX)
        .ok_or_else(|| format!("uri must start with '{URI_PREFIX}'"))?;
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
            provider,
            session_id: session_id.to_string(),
            turn,
        });
    }

    Ok(ParsedUri::Session {
        provider,
        session_id: after_provider.to_string(),
    })
}
