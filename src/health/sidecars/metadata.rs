use std::path::PathBuf;

use crate::health::{HealthCheck, HealthStatus};
use crate::metadata;

pub(in crate::health) fn metadata_db_health_check() -> HealthCheck {
    metadata_db_health_check_for_path(metadata::default_path())
}

fn metadata_db_health_check_for_path(path: Option<PathBuf>) -> HealthCheck {
    let Some(path) = path else {
        return HealthCheck {
            name: "metadata-db-readable",
            status: HealthStatus::Warn,
            message: "metadata db path could not be resolved".to_string(),
            hint: Some(
                "Set AGHIST_METADATA_DB=/path/to/metadata.db or restore HOME/XDG dirs.".to_string(),
            ),
        };
    };

    if !path.exists() {
        return HealthCheck {
            name: "metadata-db-readable",
            status: HealthStatus::Ok,
            message: format!(
                "metadata db absent; will be created on first write: {}",
                path.display()
            ),
            hint: None,
        };
    }

    match metadata::open(&path) {
        Ok(_) => HealthCheck {
            name: "metadata-db-readable",
            status: HealthStatus::Ok,
            message: format!("metadata db readable: {}", path.display()),
            hint: None,
        },
        Err(e) => HealthCheck {
            name: "metadata-db-readable",
            status: HealthStatus::Fail,
            message: format!("metadata db is not readable ({}): {e}", path.display()),
            hint: Some(
                "Back up the file, then repair it with sqlite tooling or set AGHIST_METADATA_DB to a known-good database."
                    .to_string(),
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_db_health_check_reports_corrupt_existing_db() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("metadata.db");
        std::fs::write(&db, "not sqlite").unwrap();

        let check = metadata_db_health_check_for_path(Some(db));

        assert_eq!(check.status, HealthStatus::Fail);
        assert!(check.message.contains("metadata db is not readable"));
    }
}
