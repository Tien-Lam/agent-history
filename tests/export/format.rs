use aghist::export::ExportFormat;

#[test]
fn format_from_str_roundtrip() {
    for name in &["md", "markdown", "json", "html"] {
        let fmt: ExportFormat = name.parse().unwrap();
        assert!(!fmt.label().is_empty());
        assert!(!fmt.extension().is_empty());
    }
}

#[test]
fn format_from_str_invalid() {
    assert!("pdf".parse::<ExportFormat>().is_err());
    assert!("txt".parse::<ExportFormat>().is_err());
}
