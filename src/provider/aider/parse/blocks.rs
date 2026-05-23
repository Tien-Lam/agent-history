use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};

const SESSION_HEADER: &str = "# aider chat started at ";

/// One contiguous chat block from a history file: the timestamp recovered
/// from the `# aider chat started at` header plus the body lines that follow.
pub(super) struct SessionBlock {
    pub(super) started_at: DateTime<Utc>,
    pub(super) body: String,
}

/// Splits a full history file into one [`SessionBlock`] per
/// `# aider chat started at` header. Anything before the first header is
/// dropped, and headers with unparseable timestamps drop their section.
pub(super) fn split_sessions(content: &str) -> Vec<SessionBlock> {
    let mut out: Vec<SessionBlock> = Vec::new();
    let mut current: Option<SessionBlock> = None;

    for line in content.lines() {
        if let Some(ts_str) = line.strip_prefix(SESSION_HEADER) {
            if let Some(block) = current.take() {
                out.push(block);
            }
            if let Some(started_at) = parse_session_timestamp(ts_str.trim()) {
                current = Some(SessionBlock {
                    started_at,
                    body: String::new(),
                });
            }
            continue;
        }
        if let Some(block) = current.as_mut() {
            block.body.push_str(line);
            block.body.push('\n');
        }
    }
    if let Some(block) = current {
        out.push(block);
    }
    out
}

fn parse_session_timestamp(s: &str) -> Option<DateTime<Utc>> {
    // Aider writes `YYYY-MM-DD HH:MM:SS` in local time without a tz suffix.
    // We treat it as UTC — the alternative (chrono::Local) is non-portable
    // and would make session ordering jitter across timezones. Sessions are
    // always rendered relative to one another so this is fine in practice.
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()?;
    Utc.from_utc_datetime(&naive).into()
}
