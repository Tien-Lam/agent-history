use crate::config;
use crate::federated::LOCAL_SOURCE;
use crate::model::split_source_prefix;

use super::ResolutionError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupSource<'a> {
    Any,
    Local,
    Named(&'a str),
}

impl<'a> LookupSource<'a> {
    pub fn from_optional(source: Option<&'a str>) -> Result<Self, ResolutionError> {
        source.map_or(Ok(Self::Any), Self::explicit)
    }

    pub fn explicit(source: &'a str) -> Result<Self, ResolutionError> {
        validate_lookup_source(source)?;
        Ok(if source == LOCAL_SOURCE {
            Self::Local
        } else {
            Self::Named(source)
        })
    }

    pub(super) fn matches(self, actual: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Local => actual == LOCAL_SOURCE,
            Self::Named(source) => actual == source,
        }
    }

    pub(super) fn qualify_selector(self, selector: &str) -> String {
        match self {
            Self::Named(source) => format!("{source}:{selector}"),
            Self::Any | Self::Local => selector.to_string(),
        }
    }
}

pub(super) fn split_valid_source_prefix(
    raw: &str,
) -> Result<Option<(LookupSource<'_>, &str)>, ResolutionError> {
    let (source, rest) = split_source_prefix(raw);
    if let Some(source) = source {
        Ok(Some((LookupSource::explicit(source)?, rest)))
    } else {
        Ok(None)
    }
}

fn validate_lookup_source(source: &str) -> Result<(), ResolutionError> {
    if source == LOCAL_SOURCE {
        return Ok(());
    }
    config::validate_source_name(source).map_err(ResolutionError::InvalidSourceName)
}
