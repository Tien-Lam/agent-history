use super::*;
use crate::config::Transport;

#[test]
fn source_cache_manifest_health_check_fails_corrupt_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let source = test_source("box", "host", "/history");
    let manifest_path = source.manifest_path(dir.path());
    std::fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
    std::fs::write(&manifest_path, "{not-json").unwrap();

    let check = source_cache_manifest_health_check(&[source], Some(dir.path()));

    assert_eq!(check.status, HealthStatus::Fail);
    assert!(check.message.contains("failed to parse"));
}

#[test]
fn source_cache_manifest_health_check_warns_on_stale_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let source = test_source("box", "host", "/history");
    let data_dir = source.data_dir(dir.path());
    std::fs::create_dir_all(&data_dir).unwrap();
    SourceCacheManifest {
        name: source.name.clone(),
        host: "old-host".to_string(),
        path: source.path.clone(),
        transport: source.transport,
        data_dir: data_dir.display().to_string(),
        last_pulled_at: chrono::Utc::now(),
        last_pull_dry_run: false,
        byte_count: 0,
        file_count: 0,
    }
    .save(&source.manifest_path(dir.path()))
    .unwrap();

    let check = source_cache_manifest_health_check(&[source], Some(dir.path()));

    assert_eq!(check.status, HealthStatus::Warn);
    assert!(check.message.contains("manifest stale"));
}

fn test_source(name: &str, host: &str, path: &str) -> RemoteSource {
    RemoteSource {
        name: name.to_string(),
        host: host.to_string(),
        path: path.to_string(),
        transport: Transport::Ssh,
    }
}
