use serde_json::{json, Value};

use crate::model::Provider;

pub const LIST_LIMIT_DEFAULT: usize = 20;
pub const SEARCH_LIMIT_DEFAULT: usize = 20;
pub const SEARCH_WATCH_INTERVAL_MS_DEFAULT: u64 = 2_000;
pub const SEARCH_WATCH_ITERATIONS_DEFAULT: u32 = 0;
pub const SEARCH_HYBRID_WEIGHT_DEFAULT: f32 = 0.0;
pub const SHOW_INCLUDE_CONTEXT_DEFAULT: u32 = 0;

pub const MCP_SEARCH_LIMIT_MAX: usize = 200;
pub const MCP_LIST_LIMIT_DEFAULT: usize = 50;
pub const MCP_LIST_LIMIT_MAX: usize = 1_000;
pub const MCP_INCLUDE_CONTEXT_DEFAULT: usize = 0;
pub const MCP_INCLUDE_CONTEXT_MAX: usize = 100;

fn provider_slug_pattern() -> String {
    Provider::all()
        .iter()
        .map(|provider| provider.slug())
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn source_qualified_session_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+(#[1-9][0-9]*)?$",
        provider_slug_pattern()
    )
}

pub(crate) fn source_qualified_session_only_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+$",
        provider_slug_pattern()
    )
}

pub(crate) fn source_qualified_citation_ref_pattern() -> String {
    format!(
        "^([A-Za-z0-9][A-Za-z0-9_-]*:)?({})/[^#]+#[1-9][0-9]*$",
        provider_slug_pattern()
    )
}

pub(crate) fn provider_slug_enum() -> Value {
    json!(Provider::all().iter().map(|p| p.slug()).collect::<Vec<_>>())
}

pub(crate) fn provider_slug_enum_nullable() -> Value {
    let mut slugs: Vec<Value> = Provider::all().iter().map(|p| json!(p.slug())).collect();
    slugs.push(Value::Null);
    Value::Array(slugs)
}
