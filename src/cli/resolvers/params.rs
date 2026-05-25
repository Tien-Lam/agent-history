use std::path::PathBuf;

use aghist::cli_error::ErrorEnvelope;
use aghist::schema_fragments::{
    PARAMS_JSON_MAX_BYTES, SEARCH_LIMIT_DEFAULT, SHOW_INCLUDE_CONTEXT_DEFAULT,
};
use aghist::{export, model::Provider};

use super::{parse_provider_slug, ResolvedExport, SearchArgs, ShowFormat};

/// JSON `--params` body for `aghist export`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportParams {
    format: String,
    session: String,
    #[serde(default)]
    output: Option<PathBuf>,
    #[serde(default)]
    turn_range: Option<String>,
    #[serde(default)]
    include_notes: bool,
}

/// JSON `--params` body for `aghist index`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexParams {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    force: bool,
    #[serde(default)]
    accept_download: bool,
}

/// JSON `--params` body for `aghist search`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchParams {
    query: String,
    #[serde(default = "SearchParams::default_limit")]
    limit: usize,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    json: bool,
    #[serde(default)]
    debug_search: bool,
    #[serde(default)]
    hybrid_weight: f32,
}

impl SearchParams {
    fn default_limit() -> usize {
        SEARCH_LIMIT_DEFAULT
    }
}

/// JSON `--params` body for `aghist show`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ShowParams {
    reference: String,
    #[serde(default = "ShowParams::default_format")]
    format: String,
    #[serde(default = "ShowParams::default_include_context")]
    include_context: u32,
}

impl ShowParams {
    fn default_format() -> String {
        "md".to_string()
    }

    fn default_include_context() -> u32 {
        SHOW_INCLUDE_CONTEXT_DEFAULT
    }
}

pub(super) fn resolve_export_params(json: &str) -> Result<ResolvedExport, ErrorEnvelope> {
    let p: ExportParams = parse_params(json, "export")?;
    let format = parse_params_field(&p.format, "format", str::parse::<export::ExportFormat>)?;
    Ok(ResolvedExport {
        format,
        session: p.session,
        output: p.output,
        turn_range: p.turn_range,
        include_notes: p.include_notes,
    })
}

pub(super) fn resolve_index_params(
    json: &str,
) -> Result<(Option<Provider>, bool, bool), ErrorEnvelope> {
    let p: IndexParams = parse_params(json, "index")?;
    let provider = match p.provider {
        Some(slug) => Some(parse_params_field(&slug, "provider", parse_provider_slug)?),
        None => None,
    };
    Ok((provider, p.force, p.accept_download))
}

pub(super) fn resolve_search_params(json: &str) -> Result<SearchArgs, ErrorEnvelope> {
    let p: SearchParams = parse_params(json, "search")?;
    Ok(SearchArgs {
        query: Some(p.query),
        query_file: None,
        stdin: false,
        limit: p.limit,
        cursor: p.cursor,
        json: p.json,
        debug_search: p.debug_search,
        hybrid_weight: p.hybrid_weight,
    })
}

pub(super) fn resolve_show_params(json: &str) -> Result<(String, ShowFormat, u32), ErrorEnvelope> {
    let p: ShowParams = parse_params(json, "show")?;
    let format = parse_params_field(&p.format, "format", str::parse::<ShowFormat>)?;
    Ok((p.reference, format, p.include_context))
}

fn parse_params<T: serde::de::DeserializeOwned>(json: &str, cmd: &str) -> Result<T, ErrorEnvelope> {
    if json.len() > PARAMS_JSON_MAX_BYTES {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("--params for `{cmd}` must be at most {PARAMS_JSON_MAX_BYTES} bytes"),
        )
        .with_hint(
            "Pass only the command params JSON; use file/stdin options for large text inputs.",
        ));
    }
    serde_json::from_str(json).map_err(|e| {
        ErrorEnvelope::new(
            "usage",
            format!("--params for `{cmd}` is not valid JSON: {e}"),
        )
        .with_hint("Pass a JSON object matching the subcommand schema.")
    })
}

fn parse_params_field<T, E: std::fmt::Display>(
    raw: &str,
    field: &str,
    parse: impl FnOnce(&str) -> Result<T, E>,
) -> Result<T, ErrorEnvelope> {
    parse(raw).map_err(|e| {
        ErrorEnvelope::new("usage", format!("--params field `{field}` is invalid: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_params_rejects_oversized_json_before_deserialize() {
        let oversized = " ".repeat(PARAMS_JSON_MAX_BYTES + 1);
        let err = parse_params::<serde_json::Value>(&oversized, "show")
            .expect_err("oversized --params body should be rejected before JSON parsing");

        assert_eq!(err.kind, "usage");
        assert!(err.message.contains(&PARAMS_JSON_MAX_BYTES.to_string()));
    }
}
