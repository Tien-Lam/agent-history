pub fn validate_source_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("source name must not be empty".to_string());
    }
    if trimmed != name {
        return Err("source name must not contain leading or trailing whitespace".to_string());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("source name must not be '.' or '..'".to_string());
    }
    if trimmed == crate::federated::LOCAL_SOURCE {
        return Err("source name 'local' is reserved".to_string());
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err("source name must not be empty".to_string());
    };
    if !first.is_ascii_alphanumeric() {
        return Err("source name must start with an ASCII letter or digit".to_string());
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("source name may contain only ASCII letters, digits, '-' and '_'".to_string());
    }
    Ok(())
}

pub fn validate_rsync_endpoint(value: &str, label: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if trimmed != value {
        return Err(format!(
            "{label} must not contain leading or trailing whitespace"
        ));
    }
    if trimmed.starts_with('-') {
        return Err(format!("{label} must not start with '-'"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(format!("{label} must not contain control characters"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{validate_rsync_endpoint, validate_source_name};

    fn valid_source_name_strategy() -> impl Strategy<Value = String> {
        "[A-Za-z0-9][A-Za-z0-9_-]{0,40}"
            .prop_filter("source name must not be reserved", |name| name != "local")
    }

    proptest! {
        #[test]
        fn valid_source_names_are_accepted(name in valid_source_name_strategy()) {
            prop_assert!(validate_source_name(&name).is_ok());
        }

        #[test]
        fn source_names_with_forbidden_characters_are_rejected(
            prefix in "[A-Za-z0-9][A-Za-z0-9_-]{0,12}",
            suffix in "[A-Za-z0-9_-]{0,12}",
            bad in prop::sample::select(vec!['/', '.', ':', ' ', '\t', '\n', '\\']),
        ) {
            let name = format!("{prefix}{bad}{suffix}");
            prop_assert!(validate_source_name(&name).is_err());
        }

        #[test]
        fn rsync_endpoints_reject_control_characters(
            prefix in "[A-Za-z0-9_./:@-]{1,20}",
            suffix in "[A-Za-z0-9_./:@-]{0,20}",
            control in prop::sample::select(vec!['\0', '\n', '\r', '\t']),
        ) {
            let endpoint = format!("{prefix}{control}{suffix}");
            prop_assert!(validate_rsync_endpoint(&endpoint, "--host").is_err());
        }
    }
}
