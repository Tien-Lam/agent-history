pub(super) fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub(super) fn serde_json_string(s: &str) -> String {
    format!("\"{}\"", escape_json(s))
}

pub(super) fn iso_to_millis(iso: &str) -> u64 {
    use chrono::DateTime;
    DateTime::parse_from_rfc3339(iso)
        .map_or(0, |dt| u64::try_from(dt.timestamp_millis()).unwrap_or(0))
}
