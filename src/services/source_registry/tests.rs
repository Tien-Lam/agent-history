use std::path::PathBuf;

use super::*;

fn config_path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("config.toml")
}

#[test]
fn add_persists_remote_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = config_path(&dir);

    let source = add_remote_source(
        &path,
        "box",
        "user@example.test",
        "/history",
        Transport::Rsync,
    )
    .unwrap();

    assert_eq!(source.name, "box");
    assert_eq!(source.transport, Transport::Rsync);
    let config = load_config(&path).unwrap();
    assert_eq!(config.sources, vec![source]);
}

#[test]
fn add_rejects_duplicate_name_without_overwriting() {
    let dir = tempfile::tempdir().unwrap();
    let path = config_path(&dir);
    add_remote_source(&path, "box", "host-a", "/one", Transport::Ssh).unwrap();

    let err = add_remote_source(&path, "box", "host-b", "/two", Transport::Rsync).unwrap_err();

    assert!(matches!(err, SourceRegistryError::DuplicateSource(name) if name == "box"));
    let sources = list_remote_sources(&path).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].host, "host-a");
}

#[test]
fn add_rejects_invalid_name_without_creating_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = config_path(&dir);

    let err = add_remote_source(&path, "../box", "host", "/history", Transport::Ssh).unwrap_err();

    assert!(matches!(err, SourceRegistryError::InvalidName(_)));
    assert!(!path.exists());
}

#[test]
fn add_rejects_option_like_host_without_creating_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = config_path(&dir);

    let err = add_remote_source(&path, "box", "-host", "/history", Transport::Ssh).unwrap_err();

    assert!(matches!(err, SourceRegistryError::InvalidHost(_)));
    assert!(!path.exists());
}

#[test]
fn remove_deletes_existing_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = config_path(&dir);
    add_remote_source(&path, "keep", "host", "/keep", Transport::Ssh).unwrap();
    add_remote_source(&path, "drop", "host", "/drop", Transport::Ssh).unwrap();

    let removed = remove_remote_source(&path, "drop").unwrap();

    assert_eq!(removed.name, "drop");
    let sources = list_remote_sources(&path).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].name, "keep");
}

#[test]
fn remove_unknown_source_reports_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = config_path(&dir);

    let err = remove_remote_source(&path, "missing").unwrap_err();

    assert!(matches!(err, SourceRegistryError::SourceNotFound(name) if name == "missing"));
}

#[test]
fn load_config_surfaces_parse_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = config_path(&dir);
    std::fs::write(&path, "not = [valid").unwrap();

    let err = load_config(&path).unwrap_err();

    assert!(matches!(
        err,
        SourceRegistryError::ConfigLoad(ConfigLoadError::Parse { .. })
    ));
}
