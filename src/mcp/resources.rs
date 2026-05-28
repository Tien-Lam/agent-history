use serde_json::{json, Value};

use crate::model::{Provider, Session};

mod uris;

pub(super) use uris::{parse_aghist_uri, session_uri_for_source, turn_uri_for_source, ParsedUri};
#[cfg(test)]
pub(super) use uris::{session_uri, turn_uri};

pub(super) fn resource_descriptor_with_source(s: &Session, source: &str) -> Value {
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
        "uri": session_uri_for_source(source, s.provider, &s.id.0),
        "name": title,
        "description": description,
        "mimeType": "application/json",
        "source": source,
    })
}

pub(super) fn resource_templates() -> Value {
    let provider_slugs = Provider::all()
        .iter()
        .map(|provider| provider.slug())
        .collect::<Vec<_>>()
        .join(", ");
    let session_description = format!(
        "Session metadata + bounded ordered turns. `provider` is the kebab-case slug ({provider_slugs})."
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
        },
        {
            "uriTemplate": "aghist://source/{source}/session/{provider}/{session_id}",
            "name": "Remote source session",
            "description": "Session metadata + bounded ordered turns from a registered remote source.",
            "mimeType": "application/json"
        },
        {
            "uriTemplate": "aghist://source/{source}/session/{provider}/{session_id}/turn/{turn}",
            "name": "Remote source session turn",
            "description": "A single 1-based turn within a source-qualified remote session.",
            "mimeType": "application/json"
        }
    ])
}
