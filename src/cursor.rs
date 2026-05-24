//! Opaque base64 cursors for keyset pagination of `--list` and `search`.
//!
//! Cursors are deliberately *not* offsets: callers that resume on a shifting
//! result set (newly indexed sessions, re-ranked search hits) must not
//! silently skip or duplicate items. We encode the last item's sort key
//! instead, so the next page can be derived by "items strictly after this
//! key in the canonical sort order".
//!
//! `SearchCursor` carries the full search sort key. `ListCursor` carries the
//! session's `started_at` (primary sort, descending) plus the session identity
//! key so local and remote sessions with reused ids paginate cleanly.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::search::HitKind;

const MAX_CURSOR_TOKEN_BYTES: usize = 16 * 1024;

#[derive(Debug, Error)]
pub enum CursorError {
    #[error("failed to encode cursor payload")]
    Encode(#[source] serde_json::Error),
    #[error("invalid cursor: token exceeds size limit")]
    TooLarge,
    #[error("invalid cursor: not valid base64")]
    Base64,
    #[error("invalid cursor: malformed payload")]
    Payload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchCursor {
    pub score: f32,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub session_key: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub message_key: String,
    #[serde(default)]
    pub message_id: String,
    #[serde(default = "default_search_hit_kind")]
    pub kind: HitKind,
    #[serde(default)]
    pub note_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListCursor {
    pub started_at: DateTime<Utc>,
    pub session_id: String,
    #[serde(default)]
    pub session_key: String,
}

impl SearchCursor {
    pub fn encode(&self) -> Result<String, CursorError> {
        encode(self)
    }

    pub fn decode(token: &str) -> Result<Self, CursorError> {
        decode(token)
    }
}

impl ListCursor {
    pub fn encode(&self) -> Result<String, CursorError> {
        encode(self)
    }

    pub fn decode(token: &str) -> Result<Self, CursorError> {
        decode(token)
    }
}

fn encode<T: Serialize>(value: &T) -> Result<String, CursorError> {
    let json = serde_json::to_vec(value).map_err(CursorError::Encode)?;
    let token = URL_SAFE_NO_PAD.encode(json);
    if token.len() > MAX_CURSOR_TOKEN_BYTES {
        return Err(CursorError::TooLarge);
    }
    Ok(token)
}

fn decode<T: for<'de> Deserialize<'de>>(token: &str) -> Result<T, CursorError> {
    let token = token.trim();
    if token.len() > MAX_CURSOR_TOKEN_BYTES {
        return Err(CursorError::TooLarge);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| CursorError::Base64)?;
    serde_json::from_slice(&bytes).map_err(|_| CursorError::Payload)
}

fn default_search_hit_kind() -> HitKind {
    HitKind::Message
}

#[cfg(test)]
mod tests;
