use std::path::PathBuf;

use aghist::cli_error::ErrorEnvelope;
use aghist::model::Provider;
use aghist::schema_fragments::{
    EXPORT_TURN_RANGE_MAX_BYTES, REFERENCE_MAX_BYTES, SEARCH_LIMIT_MAX, SHOW_INCLUDE_CONTEXT_MAX,
};
use aghist::todos::TodoKind;
use aghist::{config, export};

mod params;

pub(super) fn parse_transport(raw: &str) -> Result<config::Transport, String> {
    config::Transport::from_slug(raw)
        .ok_or_else(|| format!("unknown transport '{raw}'. Valid: ssh, rsync"))
}

fn missing_required_arg(arg: &str, cmd: &str) -> ErrorEnvelope {
    ErrorEnvelope::new(
        "usage",
        format!("missing required argument `{arg}` for `{cmd}`"),
    )
    .with_hint("Pass the CLI argument or provide an equivalent --params JSON body.")
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

#[derive(Debug)]
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
    let args = if let Some(json) = params {
        params::resolve_export_params(&json)
    } else {
        Ok(ResolvedExport {
            format: format.ok_or_else(|| missing_required_arg("--format", "export"))?,
            session: session.ok_or_else(|| missing_required_arg("--session", "export"))?,
            output,
            turn_range,
            include_notes,
        })
    }?;
    validate_export_args(args)
}

fn validate_export_args(args: ResolvedExport) -> Result<ResolvedExport, ErrorEnvelope> {
    if args.session.len() > REFERENCE_MAX_BYTES {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("session selector must be at most {REFERENCE_MAX_BYTES} bytes"),
        )
        .with_hint("Use a shorter session id, session ref, or unique id prefix."));
    }
    if let Some(turn_range) = args.turn_range.as_deref() {
        if turn_range.len() > EXPORT_TURN_RANGE_MAX_BYTES {
            return Err(ErrorEnvelope::new(
                "usage",
                format!("turn range must be at most {EXPORT_TURN_RANGE_MAX_BYTES} bytes"),
            )
            .with_hint("Use a range like `12:25`, `:10`, `5:`, or `7`."));
        }
    }
    Ok(args)
}

pub(crate) fn resolve_index_args(
    provider: Option<Provider>,
    force: bool,
    accept_download: bool,
    params: Option<String>,
) -> Result<(Option<Provider>, bool, bool), ErrorEnvelope> {
    if let Some(json) = params {
        params::resolve_index_params(&json)
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
    pub(crate) debug_search: bool,
    pub(crate) hybrid_weight: f32,
}

pub(crate) fn resolve_search_args(
    args: SearchArgs,
    params: Option<String>,
) -> Result<SearchArgs, ErrorEnvelope> {
    let args = if let Some(raw) = params {
        params::resolve_search_params(&raw)?
    } else {
        args
    };
    validate_search_args(args)
}

fn validate_search_args(args: SearchArgs) -> Result<SearchArgs, ErrorEnvelope> {
    if args.limit == 0 {
        return Err(
            ErrorEnvelope::new("usage", "search limit must be at least 1")
                .with_hint("Use `--limit N` or `--params {\"limit\":N}` with N >= 1."),
        );
    }
    if args.limit > SEARCH_LIMIT_MAX {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("search limit must be at most {SEARCH_LIMIT_MAX}"),
        )
        .with_hint(format!(
            "Use `--limit N` or `--params {{\"limit\":N}}` with N <= {SEARCH_LIMIT_MAX}."
        )));
    }
    if !args.hybrid_weight.is_finite() || !(0.0..=1.0).contains(&args.hybrid_weight) {
        return Err(ErrorEnvelope::new(
            "usage",
            "hybrid_weight must be a finite number between 0.0 and 1.0",
        )
        .with_hint("Use a value in the schema range: 0.0 <= hybrid_weight <= 1.0."));
    }
    Ok(args)
}

pub(crate) fn resolve_show_args(
    reference: Option<String>,
    format: ShowFormat,
    include_context: u32,
    params: Option<String>,
) -> Result<(String, ShowFormat, u32), ErrorEnvelope> {
    let args = if let Some(json) = params {
        params::resolve_show_params(&json)?
    } else {
        (
            reference.ok_or_else(|| missing_required_arg("REF", "show"))?,
            format,
            include_context,
        )
    };
    validate_show_args(args)
}

fn validate_show_args(
    args: (String, ShowFormat, u32),
) -> Result<(String, ShowFormat, u32), ErrorEnvelope> {
    if args.0.len() > REFERENCE_MAX_BYTES {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("reference must be at most {REFERENCE_MAX_BYTES} bytes"),
        )
        .with_hint("Use a shorter citation ref."));
    }
    if args.2 > SHOW_INCLUDE_CONTEXT_MAX {
        return Err(ErrorEnvelope::new(
            "usage",
            format!("include_context must be at most {SHOW_INCLUDE_CONTEXT_MAX}"),
        )
        .with_hint(format!(
            "Use `--include-context N` or `--params {{\"include_context\":N}}` with N <= {SHOW_INCLUDE_CONTEXT_MAX}."
        )));
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_export_args_reports_missing_required_cli_arg() {
        let err = resolve_export_args(None, Some("abc".to_string()), None, None, false, None)
            .expect_err("missing --format should be a usage error");

        assert_eq!(err.kind, "usage");
        assert!(err.message.contains("--format"));
        assert!(err.hint.is_some());
    }

    #[test]
    fn resolve_show_args_reports_missing_ref() {
        let err = resolve_show_args(None, ShowFormat::Md, 0, None)
            .expect_err("missing REF should be a usage error");

        assert_eq!(err.kind, "usage");
        assert!(err.message.contains("REF"));
    }

    #[test]
    fn resolve_show_args_rejects_values_above_max_context() {
        let err = resolve_show_args(
            Some("claude-code/session#1".to_string()),
            ShowFormat::Md,
            SHOW_INCLUDE_CONTEXT_MAX + 1,
            None,
        )
        .expect_err("oversized context should be rejected");

        assert_eq!(err.kind, "usage");
        assert!(err.message.contains(&SHOW_INCLUDE_CONTEXT_MAX.to_string()));
    }

    #[test]
    fn resolve_show_args_rejects_oversized_reference() {
        let reference = format!("claude-code/{}#1", "s".repeat(REFERENCE_MAX_BYTES));
        let err = resolve_show_args(Some(reference), ShowFormat::Md, 0, None)
            .expect_err("oversized show ref should be rejected before lookup");

        assert_eq!(err.kind, "usage");
        assert!(err.message.contains(&REFERENCE_MAX_BYTES.to_string()));
    }

    #[test]
    fn resolve_export_args_rejects_oversized_selector_and_turn_range() {
        let session = "s".repeat(REFERENCE_MAX_BYTES + 1);
        let err = resolve_export_args(
            Some(export::ExportFormat::Markdown),
            Some(session),
            None,
            None,
            false,
            None,
        )
        .expect_err("oversized export selector should be rejected before lookup");
        assert_eq!(err.kind, "usage");
        assert!(err.message.contains(&REFERENCE_MAX_BYTES.to_string()));

        let turn_range = "1".repeat(EXPORT_TURN_RANGE_MAX_BYTES + 1);
        let err = resolve_export_args(
            Some(export::ExportFormat::Markdown),
            Some("session-id".to_string()),
            None,
            Some(turn_range),
            false,
            None,
        )
        .expect_err("oversized export turn range should be rejected before lookup");
        assert_eq!(err.kind, "usage");
        assert!(err
            .message
            .contains(&EXPORT_TURN_RANGE_MAX_BYTES.to_string()));
    }

    #[test]
    fn resolve_search_args_rejects_values_above_max_limit() {
        let result = resolve_search_args(
            SearchArgs {
                query: Some("needle".to_string()),
                query_file: None,
                stdin: false,
                limit: SEARCH_LIMIT_MAX + 1,
                cursor: None,
                json: false,
                debug_search: false,
                hybrid_weight: 0.0,
            },
            None,
        );
        let Err(err) = result else {
            panic!("oversized search limit should be rejected");
        };

        assert_eq!(err.kind, "usage");
        assert!(err.message.contains(&SEARCH_LIMIT_MAX.to_string()));
    }
}
