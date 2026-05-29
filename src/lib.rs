#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod action;
pub mod app;
pub mod cli_error;
pub mod command_spec;
pub mod config;
pub mod cursor;
pub mod decisions;
pub mod dto;
pub mod embed;
pub mod event;
pub mod export;
pub mod federated;
pub(crate) mod fs_atomic;
pub(crate) mod fs_read;
pub mod health;
pub mod indexing;
pub mod llm;
pub mod mcp;
pub mod metadata;
pub mod model;
pub mod output;
pub mod project;
pub mod provider;
pub mod provider_diagnostic;
pub mod query_scope;
pub mod report;
pub mod schema;
pub mod schema_fragments;
pub mod search;
pub mod services;
pub mod session_resolver;
pub mod session_warnings;
pub mod stars;
pub mod threads;
pub mod todos;
pub mod ui;
pub mod usage;
