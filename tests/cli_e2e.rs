mod common;

use assert_cmd::Command;

fn aghist() -> Command {
    common::helpers::isolated_aghist("cli-e2e")
}

#[path = "cli_e2e/analytics.rs"]
mod analytics;
#[path = "cli_e2e/basics.rs"]
mod basics;
#[path = "cli_e2e/diff.rs"]
mod diff;
#[path = "cli_e2e/export_show.rs"]
mod export_show;
#[path = "cli_e2e/health.rs"]
mod health;
#[path = "cli_e2e/list_index.rs"]
mod list_index;
#[path = "cli_e2e/metadata_crud.rs"]
mod metadata_crud;
#[path = "cli_e2e/metadata_filters.rs"]
mod metadata_filters;
#[path = "cli_e2e/params_schema_filters.rs"]
mod params_schema_filters;
#[path = "cli_e2e/search.rs"]
mod search;
#[path = "cli_e2e/sources.rs"]
mod sources;
