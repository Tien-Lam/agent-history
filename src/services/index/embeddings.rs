/// Drive the (opt-in) semantic side of indexing.
///
/// Three states feed the JSON summary back to the caller:
///
/// - `disabled`: the binary was built without the `embeddings` feature, so we
///   surface that even when `--accept-download` is passed (users would
///   otherwise see silent no-ops).
/// - `awaiting-consent`: feature is compiled in, no consent file exists, and
///   `--accept-download` was not passed. Lexical indexing still happened.
/// - `enabled`: consent recorded (just now or in a prior run); embeddings
///   were generated and persisted.
#[cfg(not(feature = "embeddings"))]
pub(super) fn disabled_embeddings_summary(accept_download: bool) -> serde_json::Value {
    serde_json::json!({
        "status": "disabled",
        "reason": "binary built without `embeddings` feature",
        "accept_download_requested": accept_download,
    })
}

#[cfg(feature = "embeddings")]
mod enabled;

#[cfg(feature = "embeddings")]
pub(super) use enabled::run_embeddings;
