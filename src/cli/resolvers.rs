use std::path::PathBuf;

use aghist::cli_error::ErrorEnvelope;
use aghist::model::Provider;
use aghist::todos::TodoKind;
use aghist::{config, export};

pub(super) fn parse_transport(raw: &str) -> Result<config::Transport, String> {
    config::Transport::from_slug(raw)
        .ok_or_else(|| format!("unknown transport '{raw}'. Valid: ssh, rsync"))
}

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
    hybrid_weight: f32,
}

impl SearchParams {
    fn default_limit() -> usize {
        20
    }
}

/// JSON `--params` body for `aghist show`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ShowParams {
    reference: String,
    #[serde(default = "ShowParams::default_format")]
    format: String,
    #[serde(default)]
    include_context: u32,
}

impl ShowParams {
    fn default_format() -> String {
        "md".to_string()
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(json: &str, cmd: &str) -> Result<T, ErrorEnvelope> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShowFormat {
    Md,
    Json,
    Text,
}

impl std::str::FromStr for ShowFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "md" | "markdown" => Ok(Self::Md),
            "json" => Ok(Self::Json),
            "text" | "txt" => Ok(Self::Text),
            _ => Err(format!("unknown format '{s}' (expected: md, json, text)")),
        }
    }
}

pub(super) fn parse_todo_kind(raw: &str) -> Result<TodoKind, String> {
    TodoKind::from_slug(raw).ok_or_else(|| {
        format!(
            "unknown todo kind '{raw}'. Valid: todo, follow-up, come-back-to, we-should, bd-ref"
        )
    })
}

pub(super) fn parse_provider_slug(raw: &str) -> Result<Provider, String> {
    Provider::from_slug(raw).ok_or_else(|| {
        let valid = Provider::all()
            .iter()
            .map(|p| p.slug())
            .collect::<Vec<_>>()
            .join(", ");
        format!("unknown provider slug '{raw}'. Valid: {valid}")
    })
}

pub(super) fn parse_usage_group_by(raw: &str) -> Result<aghist::usage::GroupBy, String> {
    aghist::usage::GroupBy::parse(raw)
        .map_err(|bad| format!("unknown --by value '{bad}'. Valid: model, provider, project"))
}

pub(crate) struct ResolvedExport {
    pub(crate) format: export::ExportFormat,
    pub(crate) session: String,
    pub(crate) output: Option<PathBuf>,
    pub(crate) turn_range: Option<String>,
    pub(crate) include_notes: bool,
}

pub(crate) fn resolve_export_args(
    format: Option<export::ExportFormat>,
    session: Option<String>,
    output: Option<PathBuf>,
    turn_range: Option<String>,
    include_notes: bool,
    params: Option<String>,
) -> Result<ResolvedExport, ErrorEnvelope> {
    if let Some(json) = params {
        let p: ExportParams = parse_params(&json, "export")?;
        let format = parse_params_field(&p.format, "format", str::parse::<export::ExportFormat>)?;
        Ok(ResolvedExport {
            format,
            session: p.session,
            output: p.output,
            turn_range: p.turn_range,
            include_notes: p.include_notes,
        })
    } else {
        // clap enforces these via `required_unless_present = "params"`.
        Ok(ResolvedExport {
            format: format.expect("clap requires --format unless --params is set"),
            session: session.expect("clap requires --session unless --params is set"),
            output,
            turn_range,
            include_notes,
        })
    }
}

pub(crate) fn resolve_index_args(
    provider: Option<Provider>,
    force: bool,
    accept_download: bool,
    params: Option<String>,
) -> Result<(Option<Provider>, bool, bool), ErrorEnvelope> {
    if let Some(json) = params {
        let p: IndexParams = parse_params(&json, "index")?;
        let provider = match p.provider {
            Some(slug) => Some(parse_params_field(&slug, "provider", parse_provider_slug)?),
            None => None,
        };
        Ok((provider, p.force, p.accept_download))
    } else {
        Ok((provider, force, accept_download))
    }
}

pub(crate) struct SearchArgs {
    pub(crate) query: Option<String>,
    pub(crate) query_file: Option<PathBuf>,
    pub(crate) stdin: bool,
    pub(crate) limit: usize,
    pub(crate) cursor: Option<String>,
    pub(crate) json: bool,
    pub(crate) hybrid_weight: f32,
}

pub(crate) fn resolve_search_args(
    args: SearchArgs,
    params: Option<String>,
) -> Result<SearchArgs, ErrorEnvelope> {
    if let Some(raw) = params {
        let p: SearchParams = parse_params(&raw, "search")?;
        Ok(SearchArgs {
            query: Some(p.query),
            query_file: None,
            stdin: false,
            limit: p.limit,
            cursor: p.cursor,
            json: p.json,
            hybrid_weight: p.hybrid_weight,
        })
    } else {
        Ok(args)
    }
}

pub(crate) fn resolve_show_args(
    reference: Option<String>,
    format: ShowFormat,
    include_context: u32,
    params: Option<String>,
) -> Result<(String, ShowFormat, u32), ErrorEnvelope> {
    if let Some(json) = params {
        let p: ShowParams = parse_params(&json, "show")?;
        let format = parse_params_field(&p.format, "format", str::parse::<ShowFormat>)?;
        Ok((p.reference, format, p.include_context))
    } else {
        Ok((
            reference.expect("clap requires REF unless --params is set"),
            format,
            include_context,
        ))
    }
}
