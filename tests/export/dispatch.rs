use aghist::export::{self, ExportFormat};

use super::load_fixture_session;

#[test]
fn export_dispatch_matches_format() {
    let (session, messages) = load_fixture_session();

    let md = export::export(ExportFormat::Markdown, &session, &messages);
    assert!(md.starts_with("# "), "Markdown dispatch");

    let json = export::export(ExportFormat::Json, &session, &messages);
    assert!(json.starts_with('{'), "JSON dispatch");

    let html = export::export(ExportFormat::Html, &session, &messages);
    assert!(html.contains("<!DOCTYPE html>"), "HTML dispatch");
}
