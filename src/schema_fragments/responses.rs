mod health;
mod index;
mod list;
mod message;
mod search;

pub(crate) use health::health_response_schema;
pub(crate) use index::{index_response_schema, mcp_reindex_response_schema};
pub(crate) use list::{list_response_schema, mcp_list_response_schema, session_row_schema};
pub(crate) use message::{mcp_get_message_response_schema, mcp_get_session_response_schema};
pub(crate) use search::{mcp_search_response_schema, search_response_schema};

#[cfg(test)]
pub(crate) use list::mcp_session_row_schema;
#[cfg(test)]
pub(crate) use message::message_row_schema;
#[cfg(test)]
pub(crate) use search::search_hit_schema;
