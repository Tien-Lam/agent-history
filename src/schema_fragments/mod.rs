mod common;
mod mcp;
mod responses;

pub const LIST_LIMIT_DEFAULT: usize = 20;
pub const LIST_LIMIT_MAX: usize = 10_000;
pub const SEARCH_LIMIT_DEFAULT: usize = 20;
pub const SEARCH_LIMIT_MAX: usize = 10_000;
pub const SEARCH_QUERY_MAX_BYTES: usize = 64 * 1024;
pub const PARAMS_JSON_MAX_BYTES: usize = SEARCH_QUERY_MAX_BYTES + 4 * 1024;
pub const SEARCH_WATCH_INTERVAL_MS_DEFAULT: u64 = 2_000;
pub const SEARCH_WATCH_ITERATIONS_DEFAULT: u32 = 0;
pub const SEARCH_HYBRID_WEIGHT_DEFAULT: f32 = 0.0;
pub const SHOW_INCLUDE_CONTEXT_DEFAULT: u32 = 0;
pub const SHOW_INCLUDE_CONTEXT_MAX: u32 = 100;
pub const DIFF_CONTEXT_DEFAULT: usize = 2;
pub const DIFF_CONTEXT_MAX: usize = 10_000;
pub const REFERENCE_MAX_BYTES: usize = 4 * 1024;
pub const EXPORT_TURN_RANGE_MAX_BYTES: usize = 64;

pub const ANALYSIS_LIMIT_MAX: usize = 10_000;
pub const ANALYSIS_TRACK_LIMIT_DEFAULT: usize = 50;
pub const ANALYSIS_DECISIONS_LIMIT_DEFAULT: usize = 50;
pub const ANALYSIS_TODOS_LIMIT_DEFAULT: usize = 200;
pub const ANALYSIS_THREADS_LIMIT_DEFAULT: usize = 50;
pub const ANALYSIS_THREADS_LLM_MAX_SESSIONS_DEFAULT: usize = 200;
pub const ANALYSIS_THREADS_LLM_MAX_SESSIONS_MAX: usize = 1_000;
pub const ANALYSIS_TRACK_TOPIC_MAX_BYTES: usize = 1_024;
pub const LLM_API_KEY_MAX_BYTES: usize = 4_096;
pub const LLM_ENDPOINT_MAX_BYTES: usize = 2_048;
pub const LLM_MODEL_MAX_BYTES: usize = 256;
pub const LLM_VERSION_MAX_BYTES: usize = 128;

pub const METADATA_TAG_MAX_BYTES: usize = 1_024;
pub const METADATA_NOTE_BODY_MAX_BYTES: usize = 1024 * 1024;
pub const METADATA_NOTE_FILTER_MAX_BYTES: usize = 1_024;

pub const USAGE_LIMIT_DEFAULT: usize = 100;
pub const USAGE_LIMIT_MAX: usize = 10_000;
pub const REPORT_DAYS_MAX: i64 = 3_650;
pub const REPORT_SECTION_LIMIT_MAX: usize = 10_000;

pub const MCP_SEARCH_LIMIT_MAX: usize = 200;
pub const MCP_LIST_LIMIT_DEFAULT: usize = 50;
pub const MCP_LIST_LIMIT_MAX: usize = 1_000;
pub const MCP_INCLUDE_CONTEXT_DEFAULT: usize = 0;
pub const MCP_INCLUDE_CONTEXT_MAX: usize = 100;
pub const MCP_FILTER_STRING_MAX_BYTES: usize = 1_024;
pub const MCP_LOOKUP_STRING_MAX_BYTES: usize = 1_024;

pub(crate) use common::{
    array_schema, closed_empty_object_schema, closed_object_schema, object_schema,
    provider_slug_enum, schema_props, source_qualified_citation_ref_pattern,
    source_qualified_session_only_ref_pattern, source_qualified_session_ref_pattern,
    todo_target_ref_pattern, with_description, SchemaProperties,
};
pub(crate) use mcp::mcp_tool_contracts;
pub(crate) use responses::{
    health_response_schema, index_response_schema, list_response_schema, search_response_schema,
    session_row_schema,
};

#[cfg(test)]
pub(crate) use mcp::mcp_tool_output_schema;
#[cfg(test)]
pub(crate) use responses::{
    mcp_list_response_schema, mcp_search_response_schema, mcp_session_row_schema,
    message_row_schema, search_hit_schema,
};
