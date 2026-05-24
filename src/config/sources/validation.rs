pub const MAX_SOURCE_NAME_BYTES: usize = 64;
pub const MAX_RSYNC_ENDPOINT_BYTES: usize = 4 * 1024;

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
    if trimmed.len() > MAX_SOURCE_NAME_BYTES {
        return Err(format!(
            "source name must be at most {MAX_SOURCE_NAME_BYTES} bytes"
        ));
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
    validate_rsync_endpoint_base(value, label)?;
    reject_rsync_shell_chars(value, label)
}

pub fn validate_rsync_host(value: &str, label: &str) -> Result<(), String> {
    validate_rsync_endpoint(value, label)?;
    if value.contains(['/', '\\']) {
        return Err(format!("{label} must not contain path separators"));
    }
    if value.contains(':') {
        return Err(format!(
            "{label} must not contain ':'; configure SSH ports through your SSH config"
        ));
    }
    let at_count = value.chars().filter(|c| *c == '@').count();
    if at_count > 1 || value.starts_with('@') || value.ends_with('@') {
        return Err(format!(
            "{label} must be a host or user@host without empty segments"
        ));
    }
    Ok(())
}

pub fn validate_rsync_path(value: &str, label: &str) -> Result<(), String> {
    validate_rsync_endpoint(value, label)
}

fn validate_rsync_endpoint_base(value: &str, label: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if trimmed != value {
        return Err(format!(
            "{label} must not contain leading or trailing whitespace"
        ));
    }
    if trimmed.len() > MAX_RSYNC_ENDPOINT_BYTES {
        return Err(format!(
            "{label} must be at most {MAX_RSYNC_ENDPOINT_BYTES} bytes"
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

fn reject_rsync_shell_chars(value: &str, label: &str) -> Result<(), String> {
    if value.chars().any(char::is_whitespace) {
        return Err(format!("{label} must not contain whitespace"));
    }
    if value.chars().any(is_rsync_shell_metachar) {
        return Err(format!("{label} must not contain shell metacharacters"));
    }
    Ok(())
}

fn is_rsync_shell_metachar(c: char) -> bool {
    matches!(
        c,
        '\'' | '"'
            | '`'
            | '$'
            | ';'
            | '&'
            | '|'
            | '<'
            | '>'
            | '('
            | ')'
            | '{'
            | '}'
            | '['
            | ']'
            | '*'
            | '?'
            | '!'
            | '\\'
            | '#'
    )
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{
        validate_rsync_endpoint, validate_rsync_host, validate_rsync_path, validate_source_name,
        MAX_RSYNC_ENDPOINT_BYTES, MAX_SOURCE_NAME_BYTES,
    };

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

    #[test]
    fn rsync_endpoint_rejects_whitespace_and_shell_metacharacters() {
        for endpoint in [
            "host name",
            "host\tname",
            "/path with/spaces",
            "/path;rm",
            "/path`whoami`",
            "/path$(whoami)",
            "/path|cat",
            "/path#fragment",
        ] {
            assert!(
                validate_rsync_endpoint(endpoint, "--path").is_err(),
                "{endpoint} should be rejected"
            );
        }
    }

    #[test]
    fn rsync_host_rejects_url_and_path_shapes() {
        for host in [
            "example.test:2222",
            "example.test/path",
            "user@@example.test",
            "@example.test",
            "user@",
        ] {
            assert!(
                validate_rsync_host(host, "--host").is_err(),
                "{host} should be rejected"
            );
        }
    }

    #[test]
    fn rsync_path_accepts_common_absolute_and_relative_paths() {
        for path in ["/home/me/.claude", "module/path", "~/agent-history"] {
            assert!(
                validate_rsync_path(path, "--path").is_ok(),
                "{path} should be accepted"
            );
        }
    }

    #[test]
    fn source_name_rejects_values_above_max_length() {
        let valid = "a".repeat(MAX_SOURCE_NAME_BYTES);
        let too_long = "a".repeat(MAX_SOURCE_NAME_BYTES + 1);

        assert!(validate_source_name(&valid).is_ok());
        assert!(validate_source_name(&too_long).is_err());
    }

    #[test]
    fn rsync_endpoint_rejects_values_above_max_length() {
        let valid = "a".repeat(MAX_RSYNC_ENDPOINT_BYTES);
        let too_long = "a".repeat(MAX_RSYNC_ENDPOINT_BYTES + 1);

        assert!(validate_rsync_endpoint(&valid, "--path").is_ok());
        assert!(validate_rsync_endpoint(&too_long, "--path").is_err());
    }
}
