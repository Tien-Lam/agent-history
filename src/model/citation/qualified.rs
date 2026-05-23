use std::fmt;
use std::str::FromStr;

use serde::Serialize;

use super::base::{CitationParseError, CitationRef};

/// Split an optional source prefix from a ref.
///
/// A prefix only counts as a source when `:` appears before the provider `/`.
/// This keeps unqualified session ids containing `:` round-trippable.
pub fn split_source_prefix(raw: &str) -> (Option<&str>, &str) {
    let slash = raw.find('/');
    let colon = raw.find(':');
    match (colon, slash) {
        (Some(c), Some(s)) if c > 0 && c < s => (Some(&raw[..c]), &raw[c + 1..]),
        _ => (None, raw),
    }
}

/// A citation ref optionally qualified with a federated source name:
/// `<source>:<provider>/<session-id>#<turn>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct QualifiedCitationRef {
    pub source: Option<String>,
    pub citation: CitationRef,
}

impl QualifiedCitationRef {
    pub fn new(source: Option<String>, citation: CitationRef) -> Self {
        Self { source, citation }
    }
}

impl fmt::Display for QualifiedCitationRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(source) = &self.source {
            write!(f, "{source}:{}", self.citation)
        } else {
            self.citation.fmt(f)
        }
    }
}

impl FromStr for QualifiedCitationRef {
    type Err = CitationParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (source, raw_ref) = split_source_prefix(s);
        let citation = raw_ref.parse::<CitationRef>()?;
        Ok(Self {
            source: source.map(ToOwned::to_owned),
            citation,
        })
    }
}
