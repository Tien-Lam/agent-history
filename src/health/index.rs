use std::path::Path;

use super::{fs::check_dir_writable, HealthCheck, HealthStatus};
use crate::fs_read;

const MAX_HEALTH_MANIFEST_BYTES: usize = 64 * 1024 * 1024;

pub(super) fn index_health_checks(index_dir: &Path) -> Vec<HealthCheck> {
    vec![
        index_dir_writable_check(index_dir),
        manifest_sane_check(index_dir),
        index_schema_present_check(index_dir),
    ]
}

fn index_dir_writable_check(index_dir: &Path) -> HealthCheck {
    match check_dir_writable(index_dir) {
        Ok(()) => HealthCheck {
            name: "index-dir-writable",
            status: HealthStatus::Ok,
            message: format!("index dir writable: {}", index_dir.display()),
            hint: None,
        },
        Err(e) => HealthCheck {
            name: "index-dir-writable",
            status: HealthStatus::Fail,
            message: format!("index dir not writable ({}): {e}", index_dir.display()),
            hint: Some("Set $AGHIST_INDEX_DIR to a writable path, or fix permissions.".to_string()),
        },
    }
}

fn manifest_sane_check(index_dir: &Path) -> HealthCheck {
    let manifest_path = index_dir.join("manifest.json");
    if !manifest_path.exists() {
        return HealthCheck {
            name: "manifest-sane",
            status: HealthStatus::Warn,
            message: "no manifest.json — index has not been built".to_string(),
            hint: Some("Run `aghist index` to populate the search index.".to_string()),
        };
    }

    match fs_read::read_to_string_limited(&manifest_path, MAX_HEALTH_MANIFEST_BYTES)
        .map_err(|e| e.to_string())
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).map_err(|e| e.to_string()))
    {
        Ok(v) if v.get("sessions").is_some() => HealthCheck {
            name: "manifest-sane",
            status: HealthStatus::Ok,
            message: "manifest.json parses and has 'sessions' field".to_string(),
            hint: None,
        },
        Ok(_) => HealthCheck {
            name: "manifest-sane",
            status: HealthStatus::Warn,
            message: "manifest.json parses but is missing 'sessions' field".to_string(),
            hint: Some("Run `aghist index --force` to rebuild the manifest.".to_string()),
        },
        Err(e) => HealthCheck {
            name: "manifest-sane",
            status: HealthStatus::Fail,
            message: format!("manifest.json failed to parse: {e}"),
            hint: Some("Run `aghist index --force` to rebuild the manifest.".to_string()),
        },
    }
}

fn index_schema_present_check(index_dir: &Path) -> HealthCheck {
    if index_dir.join("meta.json").exists() {
        HealthCheck {
            name: "index-schema-present",
            status: HealthStatus::Ok,
            message: "Tantivy meta.json present".to_string(),
            hint: None,
        }
    } else {
        HealthCheck {
            name: "index-schema-present",
            status: HealthStatus::Warn,
            message: "Tantivy meta.json missing — index has not been initialised".to_string(),
            hint: Some("Run `aghist index` to create the index.".to_string()),
        }
    }
}
