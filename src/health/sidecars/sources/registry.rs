use std::collections::HashSet;

use crate::config::RemoteSource;
use crate::health::{HealthCheck, HealthStatus};

pub(in crate::health) fn source_registry_health_check(sources: &[RemoteSource]) -> HealthCheck {
    if sources.is_empty() {
        return HealthCheck {
            name: "source-registry-valid",
            status: HealthStatus::Ok,
            message: "no remote sources registered".to_string(),
            hint: None,
        };
    }

    let mut names = HashSet::new();
    let mut issues = Vec::new();
    for source in sources {
        if !names.insert(source.name.as_str()) {
            issues.push(format!("duplicate source name '{}'", source.name));
        }
        if let Err(message) = source.validate() {
            issues.push(format!("{}: {message}", source.name));
        }
    }

    if issues.is_empty() {
        HealthCheck {
            name: "source-registry-valid",
            status: HealthStatus::Ok,
            message: format!("{} remote source(s) registered and valid", sources.len()),
            hint: None,
        }
    } else {
        HealthCheck {
            name: "source-registry-valid",
            status: HealthStatus::Fail,
            message: format!(
                "remote source registry has invalid entries: {}",
                issues.join("; ")
            ),
            hint: Some(
                "Fix the [[sources]] entries in config.toml, or re-create them with `aghist sources add`."
                    .to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Transport;

    #[test]
    fn source_registry_health_check_rejects_invalid_and_duplicate_sources() {
        let sources = vec![
            test_source("box", "host", "/history"),
            test_source("box", "host2", "/other"),
            test_source("../escape", "host", "/history"),
        ];

        let check = source_registry_health_check(&sources);

        assert_eq!(check.status, HealthStatus::Fail);
        assert!(check.message.contains("duplicate source name 'box'"));
        assert!(check.message.contains("../escape"));
    }

    fn test_source(name: &str, host: &str, path: &str) -> RemoteSource {
        RemoteSource {
            name: name.to_string(),
            host: host.to_string(),
            path: path.to_string(),
            transport: Transport::Ssh,
        }
    }
}
