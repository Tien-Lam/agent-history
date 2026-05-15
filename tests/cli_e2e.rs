mod common;

use assert_cmd::Command;

fn aghist() -> Command {
    Command::cargo_bin("aghist").unwrap()
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap().flatten() {
        let ty = entry.file_type().unwrap();
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&from, &to);
        } else if ty.is_file() {
            std::fs::copy(&from, &to).unwrap();
        }
    }
}

#[path = "cli_e2e/analytics.rs"]
mod analytics;
#[path = "cli_e2e/basics.rs"]
mod basics;
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
